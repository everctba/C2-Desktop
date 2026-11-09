//! CLI do C2.
//!
//! Superficie do MVP (`c2-e7`): inicializar, indexar, inspecionar, configurar e
//! limpar. Busca e leitura (`c2 grep/read/related`) chegam nas fases seguintes.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use c2_core::error::C2Error;
use c2_core::indexer::{self, IndexOptions, IndexReport};
use c2_core::log::Logger;
use c2_core::runtime;
use c2_core::schema::MetaFile;
use c2_core::search::{self, SearchOptions};
use c2_core::{cleaner, discover, EngineState, LlmClient, Workspace};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: *mut std::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> *mut std::ffi::c_void;
}

/// Abre o desktop via shell do Windows (fora do job/console do c2).
#[cfg(windows)]
fn abrir_desktop(app: &Path, root: &Path) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    fn wide(valor: &OsStr) -> Vec<u16> {
        valor.encode_wide().chain(Some(0)).collect()
    }

    let arquivo = wide(app.as_os_str());
    let parametros = wide(OsStr::new(&format!("--root \"{}\"", root.display())));
    let operacao = wide(OsStr::new("open"));
    let resultado = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operacao.as_ptr(),
            arquivo.as_ptr(),
            parametros.as_ptr(),
            std::ptr::null(),
            1,
        )
    };
    if (resultado as isize) <= 32 {
        bail!("ShellExecuteW falhou ao abrir {}", app.display());
    }
    Ok(())
}


#[derive(Parser, Debug)]
#[command(
    name = "c2",
    version,
    disable_version_flag = true,
    about = "C2 â€” contexto semantico de projetos para agentes de IA",
    long_about = "Indexa semanticamente um projeto e permite que agentes consultem \
                  o contexto antes de ler o codigo.\n\n\
                  Fluxo: c2 init -> c2 start -> c2 grep/read -> c2 status"
)]
struct Cli {
    /// Raiz do workspace (padrao: diretorio atual ou ancestral inicializado)
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Saida em JSON
    #[arg(long, global = true)]
    json: bool,

    /// Mostra versao
    #[arg(short = 'v', visible_short_alias = 'V', long = "version", action = clap::ArgAction::Version)]
    #[allow(dead_code)]
    version_info: Option<bool>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Inicializa o workspace criando .c2/config.json
    Init {
        /// Sobrescreve a configuracao existente
        #[arg(long)]
        force: bool,
        /// Abre a janela do C2 neste projeto ao terminar
        #[arg(long)]
        open: bool,
        /// Grava o bloco de regras do agente em AGENTS.md
        #[arg(long)]
        agent: bool,
        /// Gera .mcp.json e .cursor/mcp.json (MCP project-scoped)
        #[arg(long)]
        mcp: bool,
        /// Nao abre o C2 Desktop ao terminar
        #[arg(long)]
        no_open: bool,
    },

    /// Abre a janela do C2 Desktop neste projeto
    Open,

    /// Converte metadados do prototipo bash para o schema v1 (faz backup)
    Migrate {
        /// Aplica a migracao (sem isso e dry-run)
        #[arg(long)]
        yes: bool,
    },

    /// Servidor MCP por STDIO (JSON-RPC): expoe o Core para agentes
    Mcp,

    /// Mostra as ultimas linhas do log do workspace
    Logs {
        /// Quantas linhas mostrar
        #[arg(long, default_value_t = 20)]
        lines: usize,
    },

    /// Mostra o estado do workspace
    Status,

    /// Le e altera a configuracao do workspace
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Remove os metadados .c2.json (por padrao apenas lista)
    Cleaner {
        /// Confirma a remocao
        #[arg(long)]
        yes: bool,
    },

    /// Indexa o projeto uma vez
    Reindex {
        /// Ignora o hash e reindexa tudo
        #[arg(long)]
        full: bool,
        /// Restringe a um caminho relativo
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
        /// Processa no maximo N arquivos
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Mostra o plano sem chamar o modelo nem gravar
        #[arg(long)]
        dry_run: bool,
    },

    /// Indexa continuamente (foreground; Ctrl+C encerra)
    Start {
        /// Roda so um ciclo e sai
        #[arg(long)]
        once: bool,
    },

    /// Para o engine em background
    Stop,

    /// Busca semantica nos metadados do projeto
    Grep {
        /// Pergunta em linguagem natural
        query: String,
        /// Maximo de resultados
        #[arg(long, default_value_t = 5)]
        limit: usize,
        /// Mostra apenas os caminhos
        #[arg(long)]
        files_only: bool,
        /// Mostra a composicao do score e a evidencia
        #[arg(long)]
        explain: bool,
    },

    /// Le um arquivo ou trecho (a partir de c2-e7t7)
    Read {
        /// Caminho relativo ao workspace
        file: PathBuf,
        /// Faixa de linhas (ex.: 10:40)
        #[arg(long, value_name = "A:B")]
        lines: Option<String>,
    },

    /// Lista arquivos relacionados pelo grafo (a partir de c2-e7t7)
    Related {
        /// Caminho relativo ao workspace
        file: PathBuf,
    },
    /// Anotacoes de agentes no metadado (c2-zup)
    Note {
        #[command(subcommand)]
        acao: NoteAction,
    },
}

#[derive(Subcommand, Debug)]
enum NoteAction {
    /// Anexa uma anotacao ao .c2.json de um arquivo
    Add {
        /// Caminho relativo ao workspace
        file: PathBuf,
        /// Texto da anotacao (max 2000 caracteres)
        texto: String,
        /// teleology | invariant | gotcha | correction | pending | note
        #[arg(long, default_value = "note")]
        kind: String,
        /// Tarefa/contexto (opcional)
        #[arg(long)]
        task: Option<String>,
        /// Autor (padrao: agent)
        #[arg(long)]
        by: Option<String>,
        /// Modelo do agente (opcional)
        #[arg(long)]
        model: Option<String>,
    },
    /// Lista as anotacoes
    List {
        /// Caminho relativo ao workspace (opcional)
        file: Option<PathBuf>,
    },
    /// Remove uma anotacao (indice 1-based)
    Remove {
        /// Caminho relativo ao workspace
        file: PathBuf,
        /// Indice da anotacao (1-based)
        index: usize,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Mostra o valor de uma chave (ex.: model.port)
    Get { key: String },
    /// Altera o valor de uma chave
    Set { key: String, value: String },
    /// Mostra a configuracao completa
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("c2: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init {
            force,
            open,
            agent,
            mcp,
            no_open,
        } => {
            cmd_init(cli.root.as_deref(), force, cli.json, agent, mcp)?;
            let abrir = !cli.json && !no_open;
            if abrir || open {
                if let Err(err) = cmd_open(cli.root.as_deref()) {
                    eprintln!("aviso: nao foi possivel abrir o C2 Desktop: {err:#}");
                } else {
                    println!();
                    println!("C2 Desktop aberto. Pode fechar este terminal: o app continua rodando.");
                }
            }
            Ok(())
        }
        Command::Open => cmd_open(cli.root.as_deref()),
        Command::Logs { lines } => cmd_logs(cli.root.as_deref(), lines, cli.json),
        Command::Mcp => cmd_mcp(cli.root.as_deref()),
        Command::Migrate { yes } => cmd_migrate(cli.root.as_deref(), yes, cli.json),
        Command::Status => cmd_status(cli.root.as_deref(), cli.json),
        Command::Config { action } => match action {
            ConfigAction::Get { key } => cmd_config_get(cli.root.as_deref(), key, cli.json),
            ConfigAction::Set { key, value } => {
                cmd_config_set(cli.root.as_deref(), &key, &value, cli.json)
            }
            ConfigAction::List => cmd_config_list(cli.root.as_deref(), cli.json),
        },
        Command::Cleaner { yes } => cmd_cleaner(cli.root.as_deref(), !yes, cli.json),
        Command::Reindex {
            full,
            path,
            limit,
            dry_run,
        } => cmd_reindex(cli.root.as_deref(), full, path, limit, dry_run, cli.json),
        Command::Start { once } => cmd_start(cli.root.as_deref(), once, cli.json),
        Command::Stop => cmd_stop(cli.root.as_deref()),
        Command::Grep {
            query,
            limit,
            files_only,
            explain,
        } => cmd_grep(
            cli.root.as_deref(),
            &query,
            limit,
            files_only,
            explain,
            cli.json,
        ),
        Command::Read { file, lines } => cmd_read(cli.root.as_deref(), file, lines, cli.json),
        Command::Related { file } => cmd_related(cli.root.as_deref(), file, cli.json),
        Command::Note { acao } => cmd_note(cli.root.as_deref(), acao, cli.json),
    }
}

/// Workspace do comando: a raiz explicita, a raiz descoberta ou o diretorio atual.
fn resolve_workspace(root: Option<&Path>) -> Result<Workspace> {
    if let Some(root) = root {
        let canonical = c2_core::workspace::canonical_path(root)
            .with_context(|| format!("raiz invalida: {}", root.display()))?;
        return Ok(Workspace::at(canonical));
    }

    let cwd = std::env::current_dir().context("nao foi possivel ler o diretorio atual")?;
    match Workspace::find(&cwd)? {
        Some(found) => Ok(found),
        None => Ok(Workspace::at(cwd)),
    }
}

fn require_workspace(root: Option<&Path>) -> Result<Workspace> {
    let workspace = resolve_workspace(root)?;
    if !workspace.is_initialized() {
        bail!(
            "workspace C2 nao inicializado em {} (rode `c2 init`)",
            workspace.root().display()
        );
    }
    Ok(workspace)
}

const ASCII_INTRO: &str = include_str!("../../../site/assets/ascii/logo-ascii.txt");

fn cmd_init(
    root: Option<&Path>,
    force: bool,
    json_output: bool,
    agent: bool,
    mcp: bool,
) -> Result<()> {
    if !json_output {
        println!("{ASCII_INTRO}\n");
    }
    let workspace = resolve_workspace(root)?;
    let outcome = workspace.init(force)?;
    let agents_path = if agent {
        Some(write_agents_snippet(workspace.root())?)
    } else {
        None
    };
    // --agent tambem prepara o MCP project-scoped (c2-gj6).
    let mcp_paths = if agent || mcp {
        write_mcp_configs(workspace.root())?
    } else {
        Vec::new()
    };

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "created": outcome.created,
                "root": outcome.root,
                "config_path": outcome.config_path,
                "files": outcome.files,
                "languages": outcome.languages,
                "agents_path": agents_path.as_ref().map(|caminho| caminho.display().to_string()),
                "mcp_paths": mcp_paths.iter().map(|caminho| caminho.display().to_string()).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }

    if outcome.created {
        println!("Workspace C2 criado em {}", outcome.root.display());
    } else {
        println!(
            "Workspace C2 ja existia em {} (nada foi sobrescrito; use --force)",
            outcome.root.display()
        );
    }

    if let Some(caminho) = &agents_path {
        println!();
        println!("Regras do agente gravadas em {}", caminho.display());
        println!("(bloco entre os marcadores `c2:agent:begin` e `c2:agent:end`)");
    }
    for caminho in &mcp_paths {
        println!("MCP configurado em {}", caminho.display());
    }
    if mcp_paths.is_empty() && !agent {
        println!();
        println!("Para configurar o MCP do projeto: `c2 init --mcp` (ou --agent).");
    }
    if agents_path.is_none() && !mcp {
        println!();
        println!("Para o seu agente usar os comentarios do C2, cole o bloco abaixo no");
        println!("AGENTS.md / CLAUDE.md / .cursor/rules (ou rode `c2 init --agent`):");
        println!();
        for linha in AGENT_SNIPPET.lines() {
            if linha.is_empty() {
                println!();
            } else {
                println!("  {linha}");
            }
        }
    }
    println!();
    println!("Comandos para o seu agente:");
    println!("  c2 status                         # indice atualizado?");
    println!("  c2 grep \"<tarefa>\" --limit 5       # candidatos por significado");
    println!("  c2 related <arquivo>               # dependencias no grafo");
    println!("  c2 read <arquivo> --lines A:B      # so o trecho necessario");
    println!("  c2 mcp                             # servidor MCP (STDIO)");
    println!("  c2 note add <arquivo> --text ...   # anotacao teleologica");
    Ok(())
}

/// Bloco de regras que ensina um agente a consultar os comentarios antes do codigo.
const AGENT_SNIPPET: &str = "## Contexto antes de codigo (C2)

Existe um `<arquivo>.c2.json` ao lado de cada arquivo de codigo (mesmo
diretorio, mesmo basename). Ele fornece o contexto semantico do codigo:
what/why/goal/summary/responsibility e links[].

Ordem de investigacao:
1. ANTES de ler um arquivo de codigo, leia o `<arquivo>.c2.json` ao lado.
   Essa e a regra mais importante do projeto: contexto antes de codigo.
2. `c2 status` - os comentarios estao atualizados?
3. `c2 grep \"<tarefa>\" --limit 5` - candidatos por significado.
4. `c2 related <arquivo>` - vizinhos do grafo.
5. `c2 read <arquivo> --lines A:B` - so o trecho necessario.
6. Pense teleologicamente antes de qualquer anotacao: por que este arquivo
   existe? o que quebra se eu remover isso? o que o codigo NAO revela?
   Se a resposta for nao obvia, registre com a tool MCP `c2_annotate`
   (file, text, kind) - no maximo 1-2 por tarefa, curto (max 2000 chars).
   Use `correction` se voce pensou A e era B; `pending` se depende do humano.
   Nunca anote o obvio (ex.: este arquivo calcula descontos).

Metadado e hipotese; codigo e evidencia. Se divergir, `c2 reindex --path <arquivo>`.
Nao leia o projeto inteiro: comece com --limit 5.

O projeto traz .mcp.json e .cursor/mcp.json (gerados por `c2 init --mcp`): se o
seu cliente suportar MCP, use as tools c2_grep/c2_read/c2_related/c2_map em vez
de varrer o codigo com shell.";

/// Gera `.mcp.json` (Claude Code) e `.cursor/mcp.json` (Cursor) com o
/// servidor C2 project-scoped. Preserva outros servidores e nao sobrescreve
/// uma entrada `c2` existente (o usuario pode ter customizado args).
fn write_mcp_configs(root: &Path) -> Result<Vec<PathBuf>> {
    // Se o c2 esta no PATH (instalador), o config fica portavel entre pastas;
    // sem PATH (portatil), grava o caminho absoluto do executavel atual.
    let comando = if c2_core::runtime::which("c2").is_some() {
        "c2".to_string()
    } else {
        std::env::current_exe()
            .context("nao foi possivel localizar o executavel do c2")?
            .display()
            .to_string()
    };
    let entrada = json!({
        "command": comando,
        "args": ["mcp"]
    });

    let mut gerados = Vec::new();
    for relativo in [".mcp.json", ".cursor/mcp.json"] {
        let caminho = root.join(relativo);
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)
                .with_context(|| format!("nao foi possivel criar {}", pai.display()))?;
        }
        let mut documento = if caminho.is_file() {
            let texto = std::fs::read_to_string(&caminho)
                .with_context(|| format!("nao foi possivel ler {}", caminho.display()))?;
            serde_json::from_str::<Value>(&texto).with_context(|| {
                format!(
                    "{} nao e JSON valido; nao vou sobrescrever (corrija ou apague)",
                    caminho.display()
                )
            })?
        } else {
            json!({})
        };
        if !documento.is_object() {
            return Err(C2Error::InvalidConfig(format!(
                "{} precisa conter um objeto JSON",
                caminho.display()
            ))
            .into());
        }
        let objeto = documento.as_object_mut().expect("objeto verificado");
        let servidores = objeto
            .entry("mcpServers")
            .or_insert_with(|| json!({}));
        if !servidores.is_object() {
            return Err(C2Error::InvalidConfig(format!(
                "{}: mcpServers precisa ser um objeto",
                caminho.display()
            ))
            .into());
        }
        let mapa = servidores.as_object_mut().expect("objeto verificado");
        mapa.entry("c2").or_insert(entrada.clone());
        let texto = format!("{}\n", serde_json::to_string_pretty(&documento)?);
        std::fs::write(&caminho, texto)
            .with_context(|| format!("nao foi possivel gravar {}", caminho.display()))?;
        gerados.push(caminho);
    }
    Ok(gerados)
}

/// Cria/atualiza AGENTS.md com o bloco de regras, sem duplicar.
fn write_agents_snippet(root: &Path) -> Result<PathBuf> {
    const INICIO: &str = "<!-- c2:agent:begin -->";
    const FIM: &str = "<!-- c2:agent:end -->";
    let caminho = root.join("AGENTS.md");
    let atual = std::fs::read_to_string(&caminho).unwrap_or_default();
    if atual.contains(INICIO) {
        return Ok(caminho);
    }
    let bloco = format!("{INICIO}\n{AGENT_SNIPPET}\n{FIM}");
    let novo = if atual.trim().is_empty() {
        format!("# AGENTS.md\n\n{bloco}\n")
    } else {
        format!("{}\n\n{bloco}\n", atual.trim_end())
    };
    std::fs::write(&caminho, novo)
        .with_context(|| format!("nao foi possivel gravar {}", caminho.display()))?;
    Ok(caminho)
}

/// Localiza o executavel do C2 Desktop ao lado da CLI, no PATH ou na
/// instalacao padrao (%LOCALAPPDATA%\C2).
fn find_desktop_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let atual = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
    let dir = exe.parent()?;

    let mut candidatos = vec![dir.join("..").join("C2.exe"), dir.join("C2.exe")];
    if let Some(found) = runtime::which("C2") {
        candidatos.push(found);
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        candidatos.push(PathBuf::from(base).join("C2").join("C2.exe"));
    }

    // No Windows `c2.exe` e `C2.exe` sao o mesmo arquivo: nunca "abrir" a
    // propria CLI por engano (o app tem outro binario).
    candidatos.into_iter().find(|candidato| {
        if !candidato.is_file() {
            return false;
        }
        match std::fs::canonicalize(candidato) {
            Ok(canonico) => canonico != atual,
            Err(_) => false,
        }
    })
}
/// Abre a janela do C2 no workspace atual (ou na pasta informada).
fn cmd_open(root: Option<&Path>) -> Result<()> {
    let workspace = resolve_workspace(root)?;
    let app = find_desktop_app().ok_or_else(|| {
        anyhow::anyhow!(
            "C2 Desktop nao encontrado. Instale o pacote (instalar.ps1) ou abra a \
             janela manualmente em {}",
            workspace.root().display()
        )
    })?;

    #[cfg(windows)]
    {
        abrir_desktop(&app, workspace.root())?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(&app)
            .arg("--root")
            .arg(workspace.root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("falha ao abrir {}", app.display()))?;
    }

    println!("C2 Desktop aberto em {}", workspace.root().display());
    Ok(())
}

fn cmd_status(root: Option<&Path>, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let discovery = discover::discover(workspace.root(), &config)?;
    let metadata = cleaner::metadata_files(workspace.root())?;
    let state = EngineState::load(&workspace.state_path())?;
    let engine_pid = read_lock(&workspace);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "root": workspace.root(),
                "config_path": workspace.config_path(),
                "model": {
                    "name": config.model.name,
                    "path": config.model.path,
                    "host": config.model.host,
                    "port": config.model.port,
                    "disable_thinking": config.model.disable_thinking,
                    "provider": config.model.provider.label(),
                    "base_url": config.model.base_url,
                    "api_key": c2_core::secret::load(&config.model.api_key_env)
                        .ok()
                        .flatten()
                        .is_some(),
                },
                "index": {
                    "max_file_bytes": config.index.max_file_bytes,
                    "extensions": config.index.extensions.len(),
                    "secret_policy": config.index.secret_policy,
                    "max_total_bytes_per_run": config.index.max_total_bytes_per_run,
                },
                "engine": {
                    "poll_interval_secs": config.engine.poll_interval_secs,
                    "running": engine_pid.is_some(),
                    "pid": engine_pid,
                },
                "runtime": runtime_json(&workspace, &config),
                "runtime_error": runtime::last_error(&workspace),
                "initialized": true,
                "pending": discovery.files.len().saturating_sub(metadata.len()),
                "log": {
                    "path": workspace.logs_dir().join("c2.log"),
                    "size_bytes": std::fs::metadata(workspace.logs_dir().join("c2.log")).map(|m| m.len()).unwrap_or(0),
                    "level": config.log.level,
                },
                "files": discovery.files.len(),
                "metadata": metadata.len(),
                "skipped": discovery.skipped,
                "state": state,
            }))?
        );
        return Ok(());
    }

    println!("Workspace:   {}", workspace.root().display());
    println!("Config:      {}", workspace.config_path().display());
    println!(
        "Modelo:      {} ({}:{})",
        config.model.name, config.model.host, config.model.port
    );
    if let Some(path) = &config.model.path {
        println!("GGUF:        {path}");
    }
    println!(
        "Indice:      max {} KiB por arquivo, {} extensoes, polling {}s",
        config.index.max_file_bytes / 1024,
        config.index.extensions.len(),
        config.engine.poll_interval_secs
    );
    println!("Arquivos:    {}", discovery.files.len());
    println!("Metadados:   {}", metadata.len());
    println!(
        "Pulados:     {} binarios, {} grandes, {} extensao nao suportada, {} sensiveis",
        discovery.skipped.binary,
        discovery.skipped.too_large,
        discovery.skipped.unsupported_extension,
        discovery.skipped.sensitive
    );
    println!(
        "Segredos:    politica {:?} (c2 config set index.secret_policy mask|skip)",
        config.index.secret_policy
    );
    match engine_pid {
        Some(pid) => println!("Engine:      rodando (pid {pid})"),
        None => println!("Engine:      parado"),
    }
    print_runtime_status(&workspace, &config);
    println!(
        "Pendentes:   {} (arquivos sem metadado)",
        discovery.files.len().saturating_sub(metadata.len())
    );
    let log_path = workspace.logs_dir().join("c2.log");
    println!(
        "Log:         {} ({} KiB, nivel {})",
        log_path.display(),
        std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0) / 1024,
        config.log.level
    );
    if state.queue_total > 0 {
        let novos = state.queue.iter().filter(|item| item.priority == 0).count();
        let alterados = state.queue.iter().filter(|item| item.priority == 1).count();
        println!(
            "Fila:        {} item(ns) — topo: {} novos, {} alterados",
            state.queue_total, novos, alterados
        );
    }
    if !state.history.is_empty() {
        println!("Operacoes:");
        for operacao in state.history.iter().take(3) {
            println!(
                "  {} {:<10} {}",
                format_timestamp(operacao.ts),
                operacao.kind,
                operacao.summary
            );
        }
    }
    println!(
        "Historico:   {} indexados, {} chamadas, {} tokens, {} erros (ultima varredura: {})",
        state.indexed,
        state.llm_calls,
        state.prompt_tokens + state.completion_tokens,
        state.errors,
        if state.last_scan > 0 {
            format_timestamp(state.last_scan)
        } else {
            "nunca".to_string()
        }
    );
    Ok(())
}

fn format_timestamp(secs: i64) -> String {
    let moment = std::time::UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64);
    match moment.elapsed() {
        Ok(age) => format!("ha {}s", age.as_secs()),
        Err(_) => format!("{secs}"),
    }
}

fn cmd_config_get(root: Option<&Path>, key: String, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let value = config
        .get(&key)
        .with_context(|| format!("chave desconhecida: {key}"))?;
    if json_output {
        println!("{}", json!({ key: value }));
    } else {
        println!("{value}");
    }
    Ok(())
}

fn cmd_config_set(root: Option<&Path>, key: &str, value: &str, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let mut config = workspace.load_config()?;
    config.set(key, value)?;
    workspace.save_config(&config)?;
    if json_output {
        println!("{}", json!({ "key": key, "value": config.get(key) }));
    } else {
        println!("{key} = {value}");
    }
    Ok(())
}

fn cmd_config_list(root: Option<&Path>, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    if json_output {
        println!("{}", config.to_json()?);
    } else {
        for (key, value) in [
            ("model.name", config.get("model.name")),
            ("model.path", config.get("model.path")),
            ("model.host", config.get("model.host")),
            ("model.port", config.get("model.port")),
            ("model.ctx_size", config.get("model.ctx_size")),
            ("model.temperature", config.get("model.temperature")),
            (
                "model.disable_thinking",
                config.get("model.disable_thinking"),
            ),
            ("model.max_tokens", config.get("model.max_tokens")),
            ("model.binary", config.get("model.binary")),
            ("model.allow_lmstudio", config.get("model.allow_lmstudio")),
            ("model.gpu_layers", config.get("model.gpu_layers")),
            ("index.max_file_bytes", config.get("index.max_file_bytes")),
            ("index.secret_policy", config.get("index.secret_policy")),
            (
                "index.max_total_bytes_per_run",
                config.get("index.max_total_bytes_per_run"),
            ),
            (
                "index.respect_gitignore",
                config.get("index.respect_gitignore"),
            ),
            (
                "engine.poll_interval_secs",
                config.get("engine.poll_interval_secs"),
            ),
            ("log.level", config.get("log.level")),
            ("log.keep", config.get("log.keep")),
        ] {
            let shown = value
                .filter(|v| v != "null")
                .unwrap_or_else(|| "-".to_string());
            println!("{key} = {shown}");
        }
    }
    Ok(())
}

fn cmd_cleaner(root: Option<&Path>, dry_run: bool, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let report = cleaner::clean(workspace.root(), dry_run)?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dry_run": report.dry_run,
                "found": report.found,
                "removed": report.removed,
                "failed": report.failed,
            }))?
        );
        return Ok(());
    }

    let logger = Logger::for_workspace(&workspace, &workspace.load_config()?);
    logger.info(
        "cli",
        format!(
            "cleaner: {} encontrados, {} removidos (dry_run={})",
            report.found.len(),
            report.removed.len(),
            report.dry_run
        ),
    );

    if report.dry_run {
        println!(
            "{} metadados encontrados (nada foi removido; use --yes para remover)",
            report.found.len()
        );
        for path in &report.found {
            println!("  {}", path.display());
        }
    } else {
        println!("{} metadados removidos", report.removed.len());
        for path in &report.failed {
            eprintln!("  falhou: {path}");
        }
    }
    Ok(())
}

fn cmd_reindex(
    root: Option<&Path>,
    full: bool,
    path: Option<PathBuf>,
    limit: Option<usize>,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let client = LlmClient::from_config(&config);

    let logger = Logger::for_workspace(&workspace, &config);
    logger.info(
        "cli",
        format!("reindex iniciado (full={full}, limit={limit:?}, dry_run={dry_run})"),
    );

    let options = IndexOptions {
        full,
        limit,
        only_path: path
            .as_ref()
            .map(|p| p.to_string_lossy().replace('\\', "/")),
        dry_run,
        stop: None,
    };

    // Completar links de metadados existentes nao chama o modelo: so exigimos
    // o runtime quando existe arquivo novo ou alterado para inferir.
    let precisa_modelo = indexer::plan(&workspace, &config, full, options.only_path.as_deref())?
        .iter()
        .any(|item| {
            matches!(
                item.decision,
                indexer::Decision::New | indexer::Decision::Changed
            )
        });
    let mut runtime_started = false;
    if !dry_run && precisa_modelo && c2_core::config::usa_runtime_local(&config) {
        runtime_started = !runtime::health(&config);
        runtime::ensure(&workspace, &config)?;
    }

    let report = indexer::run(&workspace, &config, &client, &options, |progress| {
        if !json_output {
            println!(
                "  [{}/{}] {:<10} {}",
                progress.index,
                progress.total,
                progress.decision.label(),
                progress.rel
            );
        }
    })?;

    if !dry_run {
        let mut state = EngineState::load(&workspace.state_path())?;
        state.merge_report(&report);
        state.save(&workspace.state_path())?;
    }

    logger.info(
        "cli",
        format!(
            "reindex concluido: {} indexados, {} inalterados, {} erros, {} chamadas",
            report.indexed, report.unchanged, report.errors, report.calls
        ),
    );
    if let Some(erro) = &report.last_error {
        logger.warn("cli", format!("ultimo erro do reindex: {erro}"));
    }

    // Comando de uma vez: se o runtime foi subido aqui, nao deixa VRAM ocupada.
    if runtime_started {
        let _ = runtime::stop(&workspace);
    }

    print_report(&report, dry_run, json_output)
}

fn cmd_start(root: Option<&Path>, once: bool, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let client = LlmClient::from_config(&config);

    // Ctrl+C pede parada; o ciclo atual termina no proximo arquivo (c2-e5t6).
    let parar = Arc::new(AtomicBool::new(false));
    {
        let parar = Arc::clone(&parar);
        if let Err(err) = ctrlc::set_handler(move || {
            parar.store(true, Ordering::SeqCst);
        }) {
            eprintln!("aviso: nao foi possivel instalar handler de Ctrl+C: {err}");
        }
    }
    let options = IndexOptions {
        stop: Some(Arc::clone(&parar)),
        ..IndexOptions::default()
    };
    let precisa_modelo = indexer::plan(&workspace, &config, false, None)?
        .iter()
        .any(|item| {
            matches!(
                item.decision,
                indexer::Decision::New | indexer::Decision::Changed
            )
        });
    if precisa_modelo && c2_core::config::usa_runtime_local(&config) {
        runtime::ensure(&workspace, &config)?;
    }

    acquire_lock(&workspace)?;
    // Em qualquer saida (Ctrl+C, erro, once) o runtime e o lock sao liberados.
    let _guarda = EngineGuard {
        workspace: &workspace,
    };
    let interval = Duration::from_secs(config.engine.poll_interval_secs.max(1));

    loop {
        if parar.load(Ordering::SeqCst) {
            println!("parada solicitada: encerrando o ciclo atual e liberando o runtime");
            break;
        }
        let report = indexer::run(&workspace, &config, &client, &options, |progress| {
            if !json_output {
                println!(
                    "  [{}/{}] {:<10} {}",
                    progress.index,
                    progress.total,
                    progress.decision.label(),
                    progress.rel
                );
            }
        })?;

        let mut state = EngineState::load(&workspace.state_path())?;
        state.merge_report(&report);
        state.save(&workspace.state_path())?;

        let logger = Logger::for_workspace(&workspace, &config);
        logger.info(
            "engine",
            format!(
                "ciclo: {} indexados, {} inalterados, {} erros, {} chamadas",
                report.indexed, report.unchanged, report.errors, report.calls
            ),
        );
        if let Some(erro) = &report.last_error {
            logger.warn("engine", format!("erro no ciclo: {erro}"));
        }

        print_report(&report, false, json_output)?;

        if once {
            break;
        }
        if c2_core::config::usa_runtime_local(&config) && !runtime::health(&config) {
            if let Ok(outcome) = runtime::ensure(&workspace, &config) {
                if !json_output {
                    println!("runtime reiniciado ({outcome:?})");
                }
            }
        }
        if !json_output {
            println!("aguardando {interval:?} (Ctrl+C para parar)...\n");
        }
        std::thread::sleep(interval);
    }

    release_lock(&workspace);
    Ok(())
}

fn cmd_grep(
    root: Option<&Path>,
    query: &str,
    limit: usize,
    files_only: bool,
    explain: bool,
    json_output: bool,
) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let options = SearchOptions {
        limit,
        include_snippets: !files_only,
    };
    let hits = search::search(&workspace, &config, query, &options)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&hits)?);
        return Ok(());
    }

    if hits.is_empty() {
        println!("nenhum resultado para: {query}");
        return Ok(());
    }

    if files_only {
        for hit in &hits {
            println!("{}", hit.file);
        }
        return Ok(());
    }

    for (index, hit) in hits.iter().enumerate() {
        println!("{}. {}   {:.2}", index + 1, hit.file, hit.score);
        if !hit.summary.is_empty() {
            println!("   {}", hit.summary);
        }
        if explain {
            println!(
                "   score: caminho {:.1} + simbolo {:.1} + semantica {:.1}",
                hit.path_score, hit.symbol_score, hit.semantic_score
            );
        }
        println!("   motivo: {}", hit.reason());
        if let Some(snippet) = &hit.snippet {
            println!("   linhas {}-{}:", snippet.line_start, snippet.line_end);
            for line in snippet.text.lines() {
                println!("     {line}");
            }
        }
        println!();
    }
    Ok(())
}

fn cmd_note(root: Option<&Path>, acao: NoteAction, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    match acao {
        NoteAction::Add {
            file,
            texto,
            kind,
            task,
            by,
            model,
        } => {
            let rel = file.to_string_lossy().replace('\\', "/");
            let nota = c2_core::notes::append(
                &workspace,
                &rel,
                &texto,
                Some(&kind),
                task.as_deref(),
                by.as_deref(),
                model.as_deref(),
            )?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&nota)?);
            } else {
                println!("anotacao adicionada em {rel} ({}) — {}", nota.kind, nota.at);
            }
        }
        NoteAction::List { file } => {
            let filtro = file
                .as_ref()
                .map(|caminho| caminho.to_string_lossy().replace('\\', "/"));
            let notas = c2_core::notes::list(&workspace, filtro.as_deref())?;
            if json_output {
                let valor: Vec<serde_json::Value> = notas
                    .iter()
                    .map(|(rel, lista)| serde_json::json!({ "file": rel, "notes": lista }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&valor)?);
            } else if notas.is_empty() {
                println!("nenhuma anotacao de agente");
            } else {
                for (rel, lista) in notas {
                    println!("{rel}");
                    for (indice, nota) in lista.iter().enumerate() {
                        println!(
                            "  {}. [{}] {} — {}",
                            indice + 1,
                            nota.kind,
                            nota.at,
                            nota.text
                        );
                    }
                }
            }
        }
        NoteAction::Remove { file, index } => {
            let rel = file.to_string_lossy().replace('\\', "/");
            let nota = c2_core::notes::remove(&workspace, &rel, index)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&nota)?);
            } else {
                println!("anotacao {index} removida de {rel}");
            }
        }
    }
    Ok(())
}

fn cmd_read(
    root: Option<&Path>,
    file: PathBuf,
    lines: Option<String>,
    json_output: bool,
) -> Result<()> {
    let workspace = require_workspace(root)?;
    let rel = file.to_string_lossy().replace('\\', "/");
    let path = workspace
        .root()
        .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let safe = workspace
        .ensure_inside(&path)
        .with_context(|| format!("arquivo fora do workspace: {rel}"))?;
    let content =
        std::fs::read_to_string(&safe).with_context(|| format!("nao foi possivel ler {rel}"))?;

    let all: Vec<&str> = content.lines().collect();
    let (start, end) = parse_line_range(lines.as_deref(), all.len())?;
    let slice = &all[start - 1..end];

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "file": rel,
                "line_start": start,
                "line_end": end,
                "lines": slice,
            }))?
        );
        return Ok(());
    }

    println!("{rel} (linhas {start}-{end} de {})", all.len());
    let width = end.to_string().len();
    for (offset, line) in slice.iter().enumerate() {
        let number = start + offset;
        println!("{number:>width$} | {line}");
    }
    Ok(())
}

/// Interpreta `A:B` (1-based, inclusivo) ou devolve a faixa padrao.
fn parse_line_range(spec: Option<&str>, total: usize) -> Result<(usize, usize)> {
    if total == 0 {
        bail!("arquivo vazio");
    }
    let (start, end) = match spec {
        None => (1, total.min(200)),
        Some(spec) => {
            let (a, b) = spec
                .split_once(':')
                .with_context(|| format!("faixa invalida: {spec} (use A:B)"))?;
            let start: usize = a
                .trim()
                .parse()
                .with_context(|| format!("inicio invalido: {a}"))?;
            let end: usize = b
                .trim()
                .parse()
                .with_context(|| format!("fim invalido: {b}"))?;
            (start, end)
        }
    };
    if start == 0 || start > end || start > total {
        bail!("faixa invalida para arquivo de {total} linhas: {start}:{end}");
    }
    Ok((start, end.min(total)))
}

fn cmd_related(root: Option<&Path>, file: PathBuf, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let rel = file.to_string_lossy().replace('\\', "/");
    let meta_path =
        c2_core::schema::meta_path_for(Path::new(&rel.replace('/', std::path::MAIN_SEPARATOR_STR)));

    let mut all: Vec<MetaFile> = Vec::new();
    for path in cleaner::metadata_files(workspace.root())? {
        if let Ok(meta) = MetaFile::read(&path) {
            all.push(meta);
        }
    }

    let target = all.iter().find(|meta| meta.file == rel).cloned();
    let Some(target) = target else {
        bail!(
            "nenhum metadado para {rel} (esperado em {}); rode `c2 reindex`",
            meta_path.display()
        );
    };

    let uses: Vec<serde_json::Value> = target
        .links
        .iter()
        .map(|link| json!({ "to": link.to, "type": link.kind }))
        .collect();

    let mut used_by: Vec<serde_json::Value> = Vec::new();
    for meta in &all {
        if meta.file == rel {
            continue;
        }
        for link in &meta.links {
            if link.to == rel {
                used_by.push(json!({ "from": meta.file, "type": link.kind }));
            }
        }
    }

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "file": rel,
                "what": target.semantic.what,
                "summary": target.semantic.summary,
                "uses": uses,
                "used_by": used_by,
            }))?
        );
        return Ok(());
    }

    println!("{rel}");
    if !target.semantic.what.is_empty() {
        println!("  {}", target.semantic.what);
    }
    println!("usa ({}):", uses.len());
    for link in &target.links {
        println!("  {:<40} {}", link.to, link.kind);
    }
    println!("usado por ({}):", used_by.len());
    for meta in &all {
        for link in &meta.links {
            if link.to == rel {
                println!("  {:<40} {}", meta.file, link.kind);
            }
        }
    }
    Ok(())
}

fn print_report(report: &IndexReport, dry_run: bool, json_output: bool) -> Result<()> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dry_run": dry_run,
                "indexed": report.indexed,
                "unchanged": report.unchanged,
                "removed": report.removed,
                "errors": report.errors,
                "deferred": report.deferred,
                "relinked": report.relinked,
                "renamed": report.renamed,
                "hash_cache_hits": report.hash_cache_hits,
                "blocked": report.blocked,
                "masked": report.masked,
                "bytes_sent": report.bytes_sent,
                "calls": report.calls,
                "prompt_tokens": report.prompt_tokens,
                "completion_tokens": report.completion_tokens,
                "latency_ms": report.latency_ms,
                "cost_usd": report.cost_usd,
                "estimated_cost_usd": report.estimated_cost_usd,
                "cost_capped": report.cost_capped,
                "last_error": report.last_error,
            }))?
        );
        return Ok(());
    }

    println!(
        "{} indexados, {} inalterados, {} removidos, {} religados, {} renomeados, {} erros, {} adiados",
        report.indexed,
        report.unchanged,
        report.removed,
        report.relinked,
        report.renamed,
        report.errors,
        report.deferred
    );
    if report.hash_cache_hits > 0 {
        println!(
            "cache: {} hash(es) reaproveitados por (tamanho, mtime)",
            report.hash_cache_hits
        );
    }
    if report.blocked > 0 || report.masked > 0 {
        println!(
            "seguranca: {} bloqueados por segredo, {} com conteudo mascarado",
            report.blocked, report.masked
        );
    }
    if report.calls > 0 {
        let per_call = report.latency_ms / u64::from(report.calls);
        println!(
            "modelo: {} chamadas, {} tokens, {} ms por arquivo",
            report.calls,
            report.prompt_tokens + report.completion_tokens,
            per_call
        );
    }
    let resumo = c2_core::cost::resumo(report);
    if !resumo.is_empty() {
        println!("{resumo}");
    }
    if let Some(err) = &report.last_error {
        eprintln!("ultimo erro: {err}");
    }
    Ok(())
}

fn lock_path(workspace: &Workspace) -> PathBuf {
    workspace.c2_dir().join("engine.lock")
}

fn read_lock(workspace: &Workspace) -> Option<u32> {
    let text = std::fs::read_to_string(lock_path(workspace)).ok()?;
    let pid = text.trim().parse::<u32>().ok()?;
    if pid_alive(pid) {
        Some(pid)
    } else {
        None
    }
}

fn acquire_lock(workspace: &Workspace) -> Result<()> {
    if let Some(pid) = read_lock(workspace) {
        bail!("ja existe um engine rodando neste workspace (pid {pid}); use `c2 stop`");
    }
    let path = lock_path(workspace);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, std::process::id().to_string())?;
    Ok(())
}

fn release_lock(workspace: &Workspace) {
    let _ = std::fs::remove_file(lock_path(workspace));
}

fn cmd_stop(root: Option<&Path>) -> Result<()> {
    let workspace = require_workspace(root)?;

    let runtime_stopped = runtime::stop(&workspace)?;
    if runtime_stopped {
        println!("runtime (llama-server) encerrado");
    }

    let Some(pid) = read_lock(&workspace) else {
        let _ = std::fs::remove_file(lock_path(&workspace));
        if !runtime_stopped {
            println!("nenhum engine rodando neste workspace");
        }
        return Ok(());
    };

    #[cfg(windows)]
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
    #[cfg(not(windows))]
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();

    match status {
        Ok(exit) if exit.success() => {
            release_lock(&workspace);
            println!("engine {pid} encerrado");
        }
        Ok(_) => bail!("nao foi possivel encerrar o engine {pid}"),
        Err(err) => bail!("falha ao encerrar o engine {pid}: {err}"),
    }
    Ok(())
}

fn pid_alive(pid: u32) -> bool {
    runtime::pid_alive(pid)
}

fn runtime_json(workspace: &Workspace, config: &c2_core::Config) -> serde_json::Value {
    let status = runtime::status(workspace, config);
    json!({
        "healthy": status.healthy,
        "pid": status.pid,
        "binary": status.binary,
        "source": status.source,
        "version": status.version,
        "url": config.model.health_url(),
    })
}

fn print_runtime_status(workspace: &Workspace, config: &c2_core::Config) {
    let status = runtime::status(workspace, config);
    let state = if status.healthy { "saudavel" } else { "parado" };
    match (&status.binary, &status.pid) {
        (Some(binary), Some(pid)) => {
            println!("Runtime:     {state} (pid {pid}) {}", binary.display())
        }
        (Some(binary), None) => println!("Runtime:     {state} ({})", binary.display()),
        (None, _) => println!("Runtime:     nao localizado"),
    }
    if let Some(source) = &status.source {
        println!("Origem:      {source}");
    }
    if let Some(version) = &status.version {
        println!("Versao:      {version}");
    }
    if let Some(erro) = &status.last_error {
        println!("Erro:        {erro}");
    }
}

fn cmd_logs(root: Option<&Path>, lines: usize, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let logger = Logger::for_workspace(&workspace, &config);
    let linhas = logger.tail(lines);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&linhas)?);
        return Ok(());
    }
    if linhas.is_empty() {
        println!(
            "log vazio em {} (nivel {}, rotacao {} KiB)",
            logger.path().display(),
            config.log.level,
            config.log.max_file_bytes / 1024
        );
        return Ok(());
    }
    for linha in linhas {
        println!("{linha}");
    }
    Ok(())
}

fn cmd_migrate(root: Option<&Path>, apply: bool, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let report = c2_core::migrate::migrate(&workspace, apply)?;

    let logger = Logger::for_workspace(&workspace, &config);
    logger.info(
        "cli",
        format!(
            "migrate: {} analisados, {} migrados, {} ja v1, {} falhas (apply={apply})",
            report.scanned,
            report.migrated,
            report.already_v1,
            report.failed.len()
        ),
    );

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!(
        "{} metadados analisados: {} migrados, {} ja no schema v1, {} falhas",
        report.scanned,
        report.migrated,
        report.already_v1,
        report.failed.len()
    );
    if !apply {
        println!("nada foi gravado (use --yes para migrar)");
    }
    if let Some(pasta) = &report.backup_dir {
        println!("backup: {}", pasta.display());
        println!("para reverter: copie os arquivos do backup de volta para o projeto");
    }
    for falha in &report.failed {
        eprintln!("  falhou: {falha}");
    }
    Ok(())
}

/// Encerra o runtime e libera o lock ao sair, inclusive em erro ou Ctrl+C.
struct EngineGuard<'a> {
    workspace: &'a Workspace,
}

impl Drop for EngineGuard<'_> {
    fn drop(&mut self) {
        let _ = runtime::stop(self.workspace);
        release_lock(self.workspace);
    }
}

#[cfg(all(test, windows))]
mod guard_tests {
    use super::*;

    #[test]
    fn guarda_encerra_runtime_e_libera_lock() {
        let base = std::env::temp_dir().join(format!(
            "c2-guard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(base.join(".c2")).unwrap();
        let workspace = Workspace::at(&base);

        // processo filho que faria o papel do llama-server
        let mut filho = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn ping");
        std::fs::write(
            workspace.c2_dir().join("runtime.pid"),
            filho.id().to_string(),
        )
        .unwrap();
        std::fs::write(lock_path(&workspace), std::process::id().to_string()).unwrap();

        {
            let _guarda = EngineGuard {
                workspace: &workspace,
            };
        }

        assert!(!lock_path(&workspace).exists(), "lock deve ser removido");
        let mut saiu = false;
        for _ in 0..20 {
            if filho.try_wait().unwrap().is_some() {
                saiu = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !saiu {
            let _ = filho.kill();
        }
        assert!(saiu, "runtime filho nao foi encerrado pela guarda");
        let _ = std::fs::remove_file(workspace.c2_dir().join("runtime.pid"));
        std::fs::remove_dir_all(&base).ok();
    }
}

fn cmd_mcp(root: Option<&Path>) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    eprintln!(
        "c2-mcp: pronto (workspace {}, protocolo {}, ferramentas: {})",
        workspace.root().display(),
        c2_core::mcp::PROTOCOL_VERSION,
        c2_core::mcp::tool_names().join(", ")
    );

    let entrada = std::io::stdin();
    for linha in entrada.lock().lines() {
        let linha = linha?;
        if linha.trim().is_empty() {
            continue;
        }
        if let Some(resposta) = c2_core::mcp::handle(&workspace, &config, &linha) {
            println!("{resposta}");
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
