//! CLI do C2.
//!
//! Superficie do MVP (`c2-e7`): inicializar, indexar, inspecionar, configurar e
//! limpar. Busca e leitura (`c2 grep/read/related`) chegam nas fases seguintes.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use c2_core::indexer::{self, IndexOptions, IndexReport};
use c2_core::schema::MetaFile;
use c2_core::search::{self, SearchOptions};
use c2_core::{cleaner, discover, EngineState, LlmClient, Workspace};
use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser, Debug)]
#[command(
    name = "c2",
    version,
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
        Command::Init { force } => cmd_init(cli.root.as_deref(), force, cli.json),
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

fn cmd_init(root: Option<&Path>, force: bool, json_output: bool) -> Result<()> {
    let workspace = resolve_workspace(root)?;
    let outcome = workspace.init(force)?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "created": outcome.created,
                "root": outcome.root,
                "config_path": outcome.config_path,
                "files": outcome.files,
                "languages": outcome.languages,
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
    println!("Configuracao: {}", outcome.config_path.display());
    println!("Arquivos descobertos: {}", outcome.files);
    if !outcome.languages.is_empty() {
        let top: Vec<String> = outcome
            .languages
            .iter()
            .rev()
            .take(8)
            .map(|(ext, count)| format!("{ext}:{count}"))
            .collect();
        println!("Linguagens: {}", top.join("  "));
    }
    println!();
    println!("Proximo passo: `c2 start` para indexar.");
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
                },
                "index": {
                    "max_file_bytes": config.index.max_file_bytes,
                    "extensions": config.index.extensions.len(),
                },
                "engine": {
                    "poll_interval_secs": config.engine.poll_interval_secs,
                    "running": engine_pid.is_some(),
                    "pid": engine_pid,
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
        "Pulados:     {} binarios, {} grandes, {} extensao nao suportada",
        discovery.skipped.binary,
        discovery.skipped.too_large,
        discovery.skipped.unsupported_extension
    );
    match engine_pid {
        Some(pid) => println!("Engine:      rodando (pid {pid})"),
        None => println!("Engine:      parado"),
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
            ("index.max_file_bytes", config.get("index.max_file_bytes")),
            (
                "index.respect_gitignore",
                config.get("index.respect_gitignore"),
            ),
            (
                "engine.poll_interval_secs",
                config.get("engine.poll_interval_secs"),
            ),
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

    let options = IndexOptions {
        full,
        limit,
        only_path: path
            .as_ref()
            .map(|p| p.to_string_lossy().replace('\\', "/")),
        dry_run,
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
    if !dry_run && precisa_modelo && !client.health() {
        bail!(
            "llama-server nao respondeu em {}. Suba o runtime com `scripts/dev-llama.ps1` \
             ou ajuste model.host/model.port.",
            config.model.health_url()
        );
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

    print_report(&report, dry_run, json_output)
}

fn cmd_start(root: Option<&Path>, once: bool, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let client = LlmClient::from_config(&config);

    let options = IndexOptions::default();
    let precisa_modelo = indexer::plan(&workspace, &config, false, None)?
        .iter()
        .any(|item| {
            matches!(
                item.decision,
                indexer::Decision::New | indexer::Decision::Changed
            )
        });
    if precisa_modelo && !client.health() {
        bail!(
            "llama-server nao respondeu em {}. Suba o runtime com `scripts/dev-llama.ps1`.",
            config.model.health_url()
        );
    }

    acquire_lock(&workspace)?;
    let interval = Duration::from_secs(config.engine.poll_interval_secs.max(1));

    loop {
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

        print_report(&report, false, json_output)?;

        if once {
            break;
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
                "calls": report.calls,
                "prompt_tokens": report.prompt_tokens,
                "completion_tokens": report.completion_tokens,
                "latency_ms": report.latency_ms,
                "last_error": report.last_error,
            }))?
        );
        return Ok(());
    }

    println!(
        "{} indexados, {} inalterados, {} removidos, {} religados, {} erros, {} adiados",
        report.indexed,
        report.unchanged,
        report.removed,
        report.relinked,
        report.errors,
        report.deferred
    );
    if report.calls > 0 {
        let per_call = report.latency_ms / u64::from(report.calls);
        println!(
            "modelo: {} chamadas, {} tokens, {} ms por arquivo",
            report.calls,
            report.prompt_tokens + report.completion_tokens,
            per_call
        );
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
    let Some(pid) = read_lock(&workspace) else {
        let _ = std::fs::remove_file(lock_path(&workspace));
        println!("nenhum engine rodando neste workspace");
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
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
