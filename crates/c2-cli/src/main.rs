//! CLI do C2.
//!
//! Superficie do MVP (`c2-e7`): inicializar, indexar, inspecionar, configurar e
//! limpar. Busca e leitura (`c2 grep/read/related`) chegam nas fases seguintes.

mod i18n;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anstream::println;
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

    /// Idioma da execucao: en ou pt-BR (default: language.json)
    #[arg(long, global = true, value_name = "LANG")]
    lang: Option<String>,

    /// Mostra versao
    #[arg(short = 'v', visible_short_alias = 'V', long = "version", action = clap::ArgAction::Version)]
    #[allow(dead_code)]
    version_info: Option<bool>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Mostra ou altera o idioma do CLI/desktop (en ou pt-BR)
    Lang {
        /// Idioma a salvar; sem valor, mostra o atual
        lang: Option<String>,
    },

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

    /// Estatisticas locais de uso (adocao, leituras e anotacoes)
    Stats,

    /// Busca decisoes/gotchas nas anotacoes de agentes
    Decisions {
        query: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Gera os vetores das notas antes de buscar
        #[arg(long)]
        index: bool,
    },

    /// Constroi/consulta a floresta teleologica
    Forest {
        #[command(subcommand)]
        action: ForestAction,
    },

    /// Runtime de embeddings (busca semantica opcional)
    Embeddings {
        #[command(subcommand)]
        action: EmbeddingAction,
    },

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
        /// Adiciona cosine de embeddings ao grep lexical
        #[arg(long)]
        hybrid: bool,
        /// Alias de --hybrid (busca semantica)
        #[arg(long)]
        semantic: bool,
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
enum EmbeddingAction {
    /// Mostra o estado do runtime de embeddings
    Status,
    /// Sobe o runtime de embeddings
    Start,
    /// Para o runtime de embeddings
    Stop,
    /// Gera o vetor de um texto (diagnostico)
    Embed { text: String },
    /// Indexa os resumos .c2.json em vetores
    Index {
        /// Ignora o cache e re-embedda todos
        #[arg(long)]
        full: bool,
    },
    /// Arquivos similares a um arquivo (por cosine)
    Similar {
        file: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Agrupa os vizinhos por domínio
        #[arg(long)]
        domains: bool,
    },
    /// Pares de metadados duplicados e metadados genéricos
    Duplicates {
        #[arg(long, default_value_t = 0.90)]
        threshold: f32,
    },
    /// Sugere links de grafo entre arquivos similares
    Links {
        #[arg(long, default_value_t = 0.90)]
        threshold: f32,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Arquivos que mudam nos mesmos commits
    Cochange {
        file: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug)]
enum ForestAction {
    /// Gera folders + forest a partir dos metadados e docs
    Build {
        /// Gera embeddings separados por secao (requer runtime)
        #[arg(long)]
        embed: bool,
    },
    /// Mostra o overview do projeto
    Overview,
    /// Mostra o resumo de um dominio/pasta
    Domain { path: String },
    /// Briefing curto para onboarding/agente
    Brief {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        task: Option<String>,
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
    i18n::iniciar(cli.lang.as_deref());
    match cli.command {
        Command::Lang { lang } => cmd_lang(lang.as_deref()),
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
                    println!(
                        "{}",
                        cor(VERDE, i18n::t("✓ C2 Desktop opened. You can close this terminal: the app keeps running.", "✓ C2 Desktop aberto. Pode fechar este terminal: o app continua rodando."))
                    );
                    println!(
                        "{}",
                        cor(CINZA, i18n::t("  Configure model, usage mode and prompt in C2 Desktop. The CLI is for agents.", "  Configure modelo, modo de uso e prompt no C2 Desktop. A CLI fica para os agentes."))
                    );
                }
            }
            Ok(())
        }
        Command::Open => cmd_open(cli.root.as_deref()),
        Command::Logs { lines } => cmd_logs(cli.root.as_deref(), lines, cli.json),
        Command::Mcp => cmd_mcp(cli.root.as_deref()),
        Command::Migrate { yes } => cmd_migrate(cli.root.as_deref(), yes, cli.json),
        Command::Status => cmd_status(cli.root.as_deref(), cli.json),
        Command::Stats => cmd_stats(cli.root.as_deref(), cli.json),
        Command::Forest { action } => cmd_forest(cli.root.as_deref(), action, cli.json),
        Command::Decisions {
            query,
            limit,
            index,
        } => cmd_decisions(
            cli.root.as_deref(),
            query.as_deref(),
            limit,
            index,
            cli.json,
        ),
        Command::Embeddings { action } => cmd_embeddings(cli.root.as_deref(), action, cli.json),
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
            hybrid,
            semantic,
        } => cmd_grep(
            cli.root.as_deref(),
            &query,
            limit,
            files_only,
            explain,
            hybrid || semantic,
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

const RESET: &str = "\x1b[0m";
const VERDE: &str = "\x1b[32m";
const VERDE_CLARO: &str = "\x1b[92m";
const CIANO: &str = "\x1b[36m";
const AZUL: &str = "\x1b[94m";
const AMARELO: &str = "\x1b[33m";
const CINZA: &str = "\x1b[90m";
const NEGRITO: &str = "\x1b[1m";

fn cor(codigo: &str, texto: &str) -> String {
    format!("{codigo}{texto}{RESET}")
}

fn negrito(texto: &str) -> String {
    format!("{NEGRITO}{texto}{RESET}")
}

const ASCII_INTRO: &str = include_str!("../../../site/assets/ascii/logo-ascii.txt");

fn cmd_lang(lang: Option<&str>) -> Result<()> {
    match lang {
        None => {
            let atual = i18n::atual().unwrap_or(i18n::Lang::PtBr);
            println!("{}: {}", i18n::t("language", "idioma"), i18n::nome(atual));
            if let Some(caminho) = i18n::caminho() {
                println!("{}: {}", i18n::t("file", "arquivo"), caminho.display());
            }
            println!(
                "{}",
                i18n::t(
                    "change with: c2 lang en | c2 lang pt-BR (or --lang en on a command)",
                    "troque com: c2 lang en | c2 lang pt-BR (ou --lang en em um comando)"
                )
            );
            Ok(())
        }
        Some(valor) => {
            let lang = i18n::parse(valor).ok_or_else(|| {
                anyhow::anyhow!(
                    "{}: {valor} ({})",
                    i18n::t("invalid language", "idioma invalido"),
                    i18n::t("use en or pt-BR", "use en ou pt-BR")
                )
            })?;
            i18n::salvar(lang).map_err(|erro| anyhow::anyhow!(erro))?;
            i18n::set(lang);
            if i18n::en() {
                println!("language: {} (saved)", i18n::nome(lang));
            } else {
                println!("idioma: {} (salvo)", i18n::nome(lang));
            }
            Ok(())
        }
    }
}

fn cmd_init(
    root: Option<&Path>,
    force: bool,
    json_output: bool,
    agent: bool,
    mcp: bool,
) -> Result<()> {
    if !json_output {
        println!("{VERDE}{ASCII_INTRO}{RESET}\n");
        println!("{}", cor(AZUL, "C2 Onboarding"));
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
        println!(
            "{}",
            cor(
                VERDE_CLARO,
                &format!(
                    "{} {}",
                    i18n::t("C2 workspace created at", "Workspace C2 criado em"),
                    outcome.root.display()
                )
            )
        );
    } else {
        println!(
            "{}",
            cor(
                AMARELO,
                &format!(
                    "{} {} ({})",
                    i18n::t(
                        "C2 workspace already existed at",
                        "Workspace C2 ja existia em"
                    ),
                    outcome.root.display(),
                    i18n::t(
                        "nothing was overwritten; use --force",
                        "nada foi sobrescrito; use --force"
                    )
                )
            )
        );
    }

    if let Some(caminho) = &agents_path {
        println!();
        println!(
            "{}",
            cor(
                CIANO,
                &format!(
                    "{} {}",
                    i18n::t("Agent rules written to", "Regras do agente gravadas em"),
                    caminho.display()
                )
            )
        );
        println!(
            "{}",
            cor(
                CINZA,
                i18n::t(
                    "(block between the `c2:agent:begin` and `c2:agent:end` markers)",
                    "(bloco entre os marcadores `c2:agent:begin` e `c2:agent:end`)"
                )
            )
        );
    }
    for caminho in &mcp_paths {
        println!(
            "{}",
            cor(
                CIANO,
                &format!(
                    "{} {}",
                    i18n::t("MCP configured at", "MCP configurado em"),
                    caminho.display()
                )
            )
        );
    }
    if mcp_paths.is_empty() && !agent {
        println!();
        println!(
            "{}",
            cor(
                CINZA,
                i18n::t(
                    "To configure project MCP: `c2 init --mcp` (or --agent).",
                    "Para configurar o MCP do projeto: `c2 init --mcp` (ou --agent)."
                )
            )
        );
    }
    if agents_path.is_none() && !mcp {
        println!();
        println!(
            "{}",
            cor(
                CIANO,
                i18n::t(
                    "For your agent to use C2 comments, paste the block below into",
                    "Para o seu agente usar os comentarios do C2, cole o bloco abaixo no"
                )
            )
        );
        println!(
            "{}",
            cor(
                CIANO,
                i18n::t(
                    "AGENTS.md / CLAUDE.md / .cursor/rules (or run `c2 init --agent`):",
                    "AGENTS.md / CLAUDE.md / .cursor/rules (ou rode `c2 init --agent`):"
                )
            )
        );
        println!();
        println!("{}", cor(CINZA, "--- BEGIN AGENTS.MD CONTENT ---"));
        for linha in agent_snippet().lines() {
            if linha.is_empty() {
                println!();
            } else if linha.starts_with("## ")
                || linha.starts_with("<EXTREMELY")
                || linha.starts_with("</EXTREMELY")
            {
                println!("  {}", cor(CIANO, linha));
            } else if linha.starts_with("Metadado é hipótese")
                || linha.starts_with("Metadata is a hypothesis")
            {
                println!("  {}", cor(AMARELO, linha));
            } else {
                println!("  {linha}");
            }
        }
        println!("{}", cor(CINZA, "--- END AGENTS.MD CONTENT ---"));
    }
    println!();
    println!(
        "{}",
        negrito(&cor(
            CIANO,
            i18n::t("Commands for your agent:", "Comandos para o seu agente:")
        ))
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 status"),
        cor(
            CINZA,
            i18n::t(
                "                         # index up to date?",
                "                         # indice atualizado?"
            )
        )
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 grep \"<tarefa>\" --limit 5"),
        cor(
            CINZA,
            i18n::t(
                "       # candidates by meaning",
                "       # candidatos por significado"
            )
        )
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 related <arquivo>"),
        cor(
            CINZA,
            i18n::t(
                "               # graph dependencies",
                "               # dependencias no grafo"
            )
        )
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 read <arquivo> --lines A:B"),
        cor(
            CINZA,
            i18n::t(
                "      # only the needed range",
                "      # so o trecho necessario"
            )
        )
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 mcp"),
        cor(
            CINZA,
            i18n::t(
                "                             # MCP server (STDIO)",
                "                             # servidor MCP (STDIO)"
            )
        )
    );
    println!(
        "  {}{}",
        cor(VERDE, "c2 note add <arquivo> --text ..."),
        cor(
            CINZA,
            i18n::t("   # teleological annotation", "   # anotacao teleologica")
        )
    );
    Ok(())
}

/// Bloco de regras que ensina um agente a consultar os comentarios antes do codigo.
const AGENT_SNIPPET: &str = r#"# C2 Usage Guide for Agents

Copie e cole este bloco no arquivo de regras do seu agente:
`AGENTS.md`, `CLAUDE.md`, `rules.md` (ou `.cursor/rules/` e
`.github/copilot-instructions.md`).

################################################################################
<EXTREMELY_IMPORTANT>

Este projeto usa C2. Cada arquivo de código tem um `<arquivo>.c2.json` ao lado
(mesmo diretório, mesmo basename) com o contexto semântico do arquivo:
what, why, goal, summary, responsibility e links[].

1. ANTES de ler um arquivo de código, leia o `<arquivo>.c2.json` ao lado.
   Essa é a regra mais importante do projeto: contexto antes de código.
2. `c2 status` — os comentários estão atualizados?
3. Use `c2 grep` para descobrir candidatos por significado e `c2 read` para
   ler só o trecho necessário.
4. Metadado é hipótese; código é evidência. Se o `.c2.json` divergir do código,
   o código vence — e o metadado precisa ser reindexado.
5. Nunca edite `.c2.json` manualmente. Use `c2_annotate` (MCP) ou `c2 reindex`.
6. Pense teleologicamente antes de qualquer anotação: por que este arquivo
   existe? O que quebra se eu remover isso? O que o código NÃO revela?
   Se a resposta não for óbvia, registre com a tool MCP `c2_annotate`
   (file, text, kind) — no máximo 1–2 por tarefa, curtas (até 2000 chars).
   Use `correction` se você pensou A e era B; `pending` se depende do humano.
   Nunca anote o óbvio (ex.: "este arquivo calcula descontos").
   Só anote o que não é derivável do código.

## Fluxo para implementar

ANTES:

1. `c2 status` — o índice está atualizado? Existe runtime/modelo configurado?
2. `c2 grep "<tarefa>" --limit 5` — candidatos por significado.
3. Leia o `.c2.json` de cada candidato.
4. `c2 related <arquivo>` — veja o que usa e o que é usado por ele.
5. `c2 read <arquivo> --lines A:B` — só o trecho relevante.

DURANTE:

- prefira os menores trechos possíveis;
- se precisar criar algo novo, procure primeiro o padrão existente via C2;
- não copie o metadado como se fosse verdade absoluta: confirme no código.

AO TERMINAR:

- valide o que o próprio projeto já oferece (testes, lint, typecheck, build),
  usando os comandos documentados no README/CI; não presuma que existem e não
  instale ferramentas só para cumprir isso;
- o engine, quando está rodando, reindexa sozinho os arquivos alterados a cada
  ciclo. Se estiver parado ou você precisar do metadado atualizado agora, rode
  `c2 reindex --path <arquivo>`;
- se o que mudou foi o prompt ou o modelo (e não o código), use
  `c2 reindex --full`: o hash não muda, mas o conteúdo gerado precisa ser
  regenerado.

O projeto traz `.mcp.json` e `.cursor/mcp.json` (gerados por `c2 init --mcp`):
se o seu cliente suportar MCP, use as tools `c2_grep`, `c2_read`,
`c2_related` e `c2_map` em vez de varrer o código com shell.

## Comandos úteis

```bash
# Visão geral do workspace, índice e runtime
c2 status
c2 status --json

# Busca semântica: descreva a responsabilidade, não o nome do arquivo
c2 grep "onde o sistema valida permissão de usuário" --limit 5
c2 grep "fila de reindexação" --limit 5 --json

# Dependências no grafo estático (usa / usado por)
c2 related src/pedido/servico.rs
c2 related src/pedido/servico.rs --json

# Ler só o trecho necessário
c2 read src/pedido/servico.rs --lines 120:180
c2 read src/pedido/servico.rs --lines 120:180 --json

# Mapa plano do projeto (uma linha por arquivo indexável)
c2 map

# Reindexar um arquivo específico ou o projeto inteiro
c2 reindex --path src/pedido/servico.rs
c2 reindex --full

# Servidor MCP do projeto (STDIO)
c2 mcp
```

Use `c2 <comando> --help` para as opções completas.

</EXTREMELY_IMPORTANT>
################################################################################
"#;

/// English version of the agent rules block.
const AGENT_SNIPPET_EN: &str = r#"# C2 Usage Guide for Agents

Copy and paste this block into your agent rules file:
`AGENTS.md`, `CLAUDE.md`, `rules.md` (or `.cursor/rules/` and
`.github/copilot-instructions.md`).

################################################################################
<EXTREMELY_IMPORTANT>

This project uses C2. Every code file has an `<file>.c2.json` next to it
(same directory, same basename) with the file's semantic context:
what, why, goal, summary, responsibility and links[].

1. BEFORE reading a code file, read the `<file>.c2.json` next to it.
   This is the most important rule of the project: context before code.
2. `c2 status` — are the comments up to date?
3. Use `c2 grep` to discover candidates by meaning and `c2 read` to read only
   the necessary range.
4. Metadata is a hypothesis; code is evidence. If the `.c2.json` diverges from
   the code, the code wins — and the metadata must be reindexed.
5. Never edit `.c2.json` by hand. Use `c2_annotate` (MCP) or `c2 reindex`.
6. Think teleologically before any annotation: why does this file exist? What
   breaks if I remove it? What does the code NOT reveal? If the answer is not
   obvious, record it with the `c2_annotate` MCP tool (file, text, kind) —
   at most 1–2 per task, short (max 2000 chars). Use `correction` if you
   thought A and it was B; `pending` if it depends on a human decision.
   Never annotate the obvious (e.g. "this file calculates discounts").
   Only annotate what is not derivable from the code.

## Implementation flow

BEFORE:

1. `c2 status` — is the index up to date? Is a runtime/model configured?
2. `c2 grep "<task>" --limit 5` — candidates by meaning.
3. Read the `.c2.json` of each candidate.
4. `c2 related <file>` — see what it uses and what uses it.
5. `c2 read <file> --lines A:B` — only the relevant range.

DURING:

- prefer the smallest possible ranges;
- if you need to create something new, first find the existing pattern via C2;
- do not treat metadata as absolute truth: confirm in the code.

WHEN DONE:

- validate what the project itself provides (tests, lint, typecheck, build)
  using the commands documented in the README/CI; do not assume they exist and
  do not install tools just to satisfy this;
- when the engine is running, it reindexes changed files automatically on each
  cycle. If it is stopped or you need updated metadata now, run
  `c2 reindex --path <file>`;
- if the prompt or model changed (not the code), use `c2 reindex --full`:
  the hash does not change, but the generated content must be regenerated.

The project includes `.mcp.json` and `.cursor/mcp.json` (generated by
`c2 init --mcp`): if your client supports MCP, use the `c2_grep`, `c2_read`,
`c2_related` and `c2_map` tools instead of scanning code with the shell.

## Useful commands

```bash
# Workspace, index and runtime overview
c2 status
c2 status --json

# Semantic search: describe the responsibility, not the file name
c2 grep "where the system validates user permission" --limit 5
c2 grep "reindex queue" --limit 5 --json

# Dependencies in the static graph (uses / used by)
c2 related src/order/service.rs
c2 related src/order/service.rs --json

# Read only the necessary range
c2 read src/order/service.rs --lines 120:180
c2 read src/order/service.rs --lines 120:180 --json

# Flat project map (one line per indexable file)
c2 map

# Reindex one file or the whole project
c2 reindex --path src/order/service.rs
c2 reindex --full

# Project MCP server (STDIO)
c2 mcp
```

Use `c2 <command> --help` for the full options.

</EXTREMELY_IMPORTANT>
################################################################################
"#;

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
        let servidores = objeto.entry("mcpServers").or_insert_with(|| json!({}));
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
fn agent_snippet() -> &'static str {
    if i18n::en() {
        AGENT_SNIPPET_EN
    } else {
        AGENT_SNIPPET
    }
}

fn write_agents_snippet(root: &Path) -> Result<PathBuf> {
    const INICIO: &str = "<!-- c2:agent:begin -->";
    const FIM: &str = "<!-- c2:agent:end -->";
    let caminho = root.join("AGENTS.md");
    let atual = std::fs::read_to_string(&caminho).unwrap_or_default();
    if atual.contains(INICIO) {
        return Ok(caminho);
    }
    let bloco = format!("{INICIO}\n{}\n{FIM}", agent_snippet());
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
    if c2_core::instance::try_focus(Some(workspace.root())) {
        println!(
            "{}",
            i18n::t(
                "C2 Desktop was already open; existing window focused.",
                "C2 Desktop ja estava aberto; janela existente focada."
            )
        );
        return Ok(());
    }
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

    println!(
        "{} {}",
        i18n::t("C2 Desktop opened at", "C2 Desktop aberto em"),
        workspace.root().display()
    );
    Ok(())
}

fn cmd_embeddings(root: Option<&Path>, action: EmbeddingAction, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let mut config = workspace.load_config()?;
    match action {
        EmbeddingAction::Status => {
            let status = c2_core::embedding_runtime::status(&workspace, &config);
            if json_output {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
        }
        EmbeddingAction::Start => {
            c2_core::embedding_runtime::ensure(&workspace, &config)?;
            println!("runtime de embeddings pronto");
        }
        EmbeddingAction::Stop => {
            let parou = c2_core::embedding_runtime::stop(&workspace)?;
            println!(
                "{}",
                if parou {
                    "runtime de embeddings parado"
                } else {
                    "runtime de embeddings nao estava rodando"
                }
            );
        }
        EmbeddingAction::Index { full } => {
            let report =
                c2_core::embedding_index::run(&workspace, &config, full, |_, idx, total| {
                    if total > 0 && (idx == total || idx % 25 == 0) {
                        eprintln!("embeddings: {idx}/{total}");
                    }
                })?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "embeddings: {} arquivos, {} gerados, {} em cache, {} pulados, {} erros",
                    report.files, report.embedded, report.cached, report.skipped, report.errors
                );
                if let Some(erro) = &report.last_error {
                    println!("ultimo erro: {erro}");
                }
            }
        }
        EmbeddingAction::Similar {
            file,
            limit,
            domains,
        } => {
            let vizinhos = c2_core::embedding_insights::similar(&workspace, &config, &file, limit)?;
            if json_output {
                if domains {
                    let grupos = c2_core::embedding_insights::domains(&vizinhos);
                    println!("{}", serde_json::to_string_pretty(&grupos)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&vizinhos)?);
                }
            } else if vizinhos.is_empty() {
                println!("nenhum similar para {file}");
            } else {
                for vizinho in &vizinhos {
                    println!("{:.3}  {}", vizinho.score, vizinho.file);
                }
                if domains {
                    for (dominio, arquivos) in c2_core::embedding_insights::domains(&vizinhos) {
                        println!("[{dominio}] {}", arquivos.join(", "));
                    }
                }
            }
        }
        EmbeddingAction::Duplicates { threshold } => {
            let report = c2_core::embedding_insights::duplicates(&workspace, &config, threshold)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "duplicados: {} pares; genericos: {} arquivos",
                    report.pairs.len(),
                    report.generic.len()
                );
                for pair in report.pairs.iter().take(20) {
                    println!(
                        "{:.3}  {} <> {}{}",
                        pair.score,
                        pair.a,
                        pair.b,
                        if pair.linked { " (ligado)" } else { "" }
                    );
                }
            }
        }
        EmbeddingAction::Links { threshold, limit } => {
            let sugestoes = c2_core::embedding_insights::link_suggestions(
                &workspace, &config, threshold, limit,
            )?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&sugestoes)?);
            } else {
                for sugestao in &sugestoes {
                    println!(
                        "{:.3}  {} -> {}",
                        sugestao.score, sugestao.from, sugestao.to
                    );
                }
            }
        }
        EmbeddingAction::Cochange { file, limit } => {
            let arquivos = c2_core::embedding_insights::cochange(&workspace, &file, limit)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&arquivos)?);
            } else {
                for item in &arquivos {
                    println!("{} commits  {}", item.commits, item.file);
                }
            }
        }
        EmbeddingAction::Embed { text } => {
            if !config.index.embeddings.enabled {
                config.index.embeddings.enabled = true;
            }
            c2_core::embedding_runtime::ensure(&workspace, &config)?;
            let vetor = c2_core::embedding_runtime::embed(&config, &text)?;
            let norma = vetor.iter().map(|v| v * v).sum::<f32>().sqrt();
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "dim": vetor.len(),
                        "norm": norma,
                        "vector": vetor,
                    }))?
                );
            } else {
                println!("embedding: {} dimensoes, norma {:.4}", vetor.len(), norma);
            }
        }
    }
    Ok(())
}

fn cmd_decisions(
    root: Option<&Path>,
    query: Option<&str>,
    limit: usize,
    index: bool,
    json_output: bool,
) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    if index {
        let gerados = c2_core::notes_memory::index(&workspace, &config)?;
        if !json_output {
            println!("notas: {gerados} vetores gerados");
        }
    }
    let hits = c2_core::notes_memory::search(&workspace, &config, query.unwrap_or(""), limit)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&hits)?);
        return Ok(());
    }
    if hits.is_empty() {
        println!("nenhuma nota encontrada");
        return Ok(());
    }
    for hit in &hits {
        let cosine = hit
            .cosine
            .map(|v| format!(" · sem {v:.2}"))
            .unwrap_or_default();
        println!(
            "{:.2}{cosine}  [{}] {} — {}\n   {}",
            hit.score, hit.kind, hit.file, hit.at, hit.text
        );
    }
    Ok(())
}

fn cmd_stats(root: Option<&Path>, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let eventos = c2_core::usage::events(&workspace, 5000)?;
    let resumo = c2_core::usage::summarize(&eventos);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&resumo)?);
        return Ok(());
    }
    println!(
        "calls {} · code_reads {} · meta_reads {} · annotations {} · preflight_before_code {}",
        resumo.calls,
        resumo.code_reads,
        resumo.meta_reads,
        resumo.annotations,
        resumo.preflight_before_code
    );
    for (ator, quantidade) in &resumo.by_actor {
        println!("{ator}: {quantidade}");
    }
    for (ferramenta, quantidade) in &resumo.by_tool {
        println!("{ferramenta}: {quantidade}");
    }
    Ok(())
}

fn cmd_forest(root: Option<&Path>, action: ForestAction, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    match action {
        ForestAction::Build { embed } => {
            let config = workspace.load_config()?;
            let report = c2_core::forest::build(&workspace, &config)?;
            let embeddings = if embed {
                c2_core::forest::embed_sections(&workspace, &config)?
            } else {
                0
            };
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "forest: {} folders ({} reaproveitados), {} arquivos, {} notas, {} docs",
                    report.folders, report.folders_reused, report.files, report.notes, report.docs
                );
                if embed {
                    println!("embeddings: {embeddings} secoes geradas");
                }
                println!(
                    "forest: {}",
                    c2_core::forest::forest_path(&workspace).display()
                );
            }
        }
        ForestAction::Overview => {
            let forest = c2_core::forest::load(&workspace)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&forest)?);
            } else {
                println!("# {}", forest.teleology.what);
                println!("{}", forest.teleology.why);
                println!("objetivo: {}", forest.teleology.goal);
                for dominio in &forest.domains {
                    println!("- {}: {}", dominio.path, dominio.what);
                }
                if !forest.risks.is_empty() {
                    println!("\nriscos/gotchas:");
                    for risco in &forest.risks {
                        println!("- {}: {}", risco.title, risco.summary);
                    }
                }
            }
        }
        ForestAction::Domain { path } => {
            let folder = c2_core::forest::load_folder(&workspace, &path)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&folder)?);
            } else {
                println!("{}", folder.path);
                println!("what: {}", folder.teleology.what);
                println!("why: {}", folder.teleology.why);
                println!("goal: {}", folder.teleology.goal);
                println!("key files: {}", folder.key_files.join(", "));
            }
        }
        ForestAction::Brief { domain, task } => {
            println!(
                "{}",
                c2_core::forest::brief(&workspace, domain.as_deref(), task.as_deref())?
            );
        }
    }
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

    println!(
        "{:<12} {}",
        i18n::t("Workspace:", "Workspace:"),
        workspace.root().display()
    );
    println!(
        "{:<12} {}",
        i18n::t("Config:", "Config:"),
        workspace.config_path().display()
    );
    println!(
        "{:<12} {} ({}:{})",
        i18n::t("Model:", "Modelo:"),
        config.model.name,
        config.model.host,
        config.model.port
    );
    if let Some(path) = &config.model.path {
        println!("GGUF:        {path}");
    }
    println!(
        "{:<12} max {} KiB/file, {} extensions, polling {}s",
        i18n::t("Index:", "Indice:"),
        config.index.max_file_bytes / 1024,
        config.index.extensions.len(),
        config.engine.poll_interval_secs
    );
    println!(
        "{:<12} {}",
        i18n::t("Files:", "Arquivos:"),
        discovery.files.len()
    );
    println!(
        "{:<12} {}",
        i18n::t("Metadata:", "Metadados:"),
        metadata.len()
    );
    println!(
        "{:<12} {} binary, {} large, {} unsupported extension, {} sensitive",
        i18n::t("Skipped:", "Pulados:"),
        discovery.skipped.binary,
        discovery.skipped.too_large,
        discovery.skipped.unsupported_extension,
        discovery.skipped.sensitive
    );
    println!(
        "{:<12} policy {:?} (c2 config set index.secret_policy mask|skip)",
        i18n::t("Secrets:", "Segredos:"),
        config.index.secret_policy
    );
    match engine_pid {
        Some(pid) => println!(
            "{:<12} {} (pid {pid})",
            i18n::t("Engine:", "Engine:"),
            i18n::t("running", "rodando")
        ),
        None => println!(
            "{:<12} {}",
            i18n::t("Engine:", "Engine:"),
            i18n::t("stopped", "parado")
        ),
    }
    print_runtime_status(&workspace, &config);
    println!(
        "{:<12} {} (files without metadata)",
        i18n::t("Pending:", "Pendentes:"),
        discovery.files.len().saturating_sub(metadata.len())
    );
    let log_path = workspace.logs_dir().join("c2.log");
    println!(
        "{:<12} {} ({} KiB, level {})",
        i18n::t("Log:", "Log:"),
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
        println!("{}", i18n::t("Operations:", "Operacoes:"));
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
    hybrid: bool,
    json_output: bool,
) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let options = SearchOptions {
        limit,
        include_snippets: !files_only,
        hybrid,
    };
    let hits = search::search(&workspace, &config, query, &options)?;
    let _ = c2_core::usage::record(
        &workspace,
        &c2_core::usage::UsageEvent::new("cli", "c2_grep").result_count(hits.len()),
    );

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
            let _ = c2_core::usage::record(
                &workspace,
                &c2_core::usage::UsageEvent::new("cli", "c2_annotate")
                    .file(&rel)
                    .meta_reads(1)
                    .annotation(&kind),
            );
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
    let _ = c2_core::usage::record(
        &workspace,
        &c2_core::usage::UsageEvent::new("cli", "c2_read")
            .file(&rel)
            .meta_reads(1),
    );

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
    let state = if status.healthy {
        i18n::t("healthy", "saudavel")
    } else {
        i18n::t("stopped", "parado")
    };
    match (&status.binary, &status.pid) {
        (Some(binary), Some(pid)) => {
            println!(
                "{:<12} {state} (pid {pid}) {}",
                i18n::t("Runtime:", "Runtime:"),
                binary.display()
            )
        }
        (Some(binary), None) => println!(
            "{:<12} {state} ({})",
            i18n::t("Runtime:", "Runtime:"),
            binary.display()
        ),
        (None, _) => println!(
            "{:<12} {}",
            i18n::t("Runtime:", "Runtime:"),
            i18n::t("not found", "nao localizado")
        ),
    }
    if let Some(source) = &status.source {
        println!("{:<12} {source}", i18n::t("Source:", "Origem:"));
    }
    if let Some(version) = &status.version {
        println!("{:<12} {version}", i18n::t("Version:", "Versao:"));
    }
    if let Some(erro) = &status.last_error {
        println!("{:<12} {erro}", i18n::t("Error:", "Erro:"));
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
        "{} (workspace {}, protocol {}, tools: {})",
        i18n::t("c2-mcp: ready", "c2-mcp: pronto"),
        workspace.root().display(),
        c2_core::mcp::PROTOCOL_VERSION,
        c2_core::mcp::tool_names().join(", ")
    );
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprintln!(
            "{}",
            i18n::t("c2-mcp: warning: this is an STDIO server for MCP clients; do not type commands here.", "c2-mcp: aviso: este e um servidor STDIO para clientes MCP; nao digite comandos aqui.")
        );
        eprintln!(
            "{}",
            i18n::t("        to start the engine, open C2 Desktop (or another terminal with `c2 start`); Ctrl+C stops the server.", "        para iniciar o engine, abra o C2 Desktop (ou outro terminal com `c2 start`); Ctrl+C encerra o servidor.")
        );
        eprintln!(
            "{}",
            i18n::t("        automatic indexing depends on C2 Desktop being open; this server is read-only.", "        a indexacao automatica depende do C2 Desktop aberto; este servidor e somente leitura.")
        );
    }

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
