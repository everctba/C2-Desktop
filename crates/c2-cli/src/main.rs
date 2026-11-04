//! CLI do C2.
//!
//! Superficie minima do MVP (`c2-e7`): inicializar, inspecionar, configurar e
//! limpar. Comandos de indexacao e busca entram nas fases seguintes e hoje
//! respondem com o bead correspondente.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use c2_core::{cleaner, discover, Config, Workspace};
use clap::{Parser, Subcommand};
use serde_json::json;

/// Codigo de saida para comando ainda nao implementado.
const EXIT_NOT_IMPLEMENTED: u8 = 3;

#[derive(Parser, Debug)]
#[command(
    name = "c2",
    version,
    about = "C2 Ã¢â‚¬â€ contexto semantico de projetos para agentes de IA",
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

    /// Reindexa o projeto (a partir de c2-e5t4)
    Reindex {
        /// Ignora o cache de hash
        #[arg(long)]
        full: bool,
        /// Restringe a um caminho
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
    },

    /// Inicia o engine e o llama-server (a partir de c2-e7t3)
    Start,

    /// Para o engine e o llama-server (a partir de c2-e7t3)
    Stop,

    /// Busca semantica (a partir de c2-e8)
    Grep {
        /// Pergunta em linguagem natural
        query: String,
        /// Maximo de resultados
        #[arg(long, default_value_t = 5)]
        limit: usize,
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
        Command::Reindex { .. } => not_implemented("c2 reindex", "c2-e5t4"),
        Command::Start => not_implemented("c2 start", "c2-e7t3"),
        Command::Stop => not_implemented("c2 stop", "c2-e7t3"),
        Command::Grep { .. } => not_implemented("c2 grep", "c2-e8"),
        Command::Read { .. } => not_implemented("c2 read", "c2-e7t7"),
        Command::Related { .. } => not_implemented("c2 related", "c2-e7t7"),
    }
}

fn not_implemented(command: &str, bead: &str) -> Result<()> {
    eprintln!("{command}: ainda nao implementado Ã¢â‚¬â€ ver bead {bead} (`bd show {bead}`)");
    std::process::exit(EXIT_NOT_IMPLEMENTED as i32);
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
    println!("Proximo passo: `c2 start` para indexar (a partir de c2-e7t3).");
    Ok(())
}

fn cmd_status(root: Option<&Path>, json_output: bool) -> Result<()> {
    let workspace = require_workspace(root)?;
    let config = workspace.load_config()?;
    let discovery = discover::discover(workspace.root(), &config)?;
    let metadata = cleaner::metadata_files(workspace.root())?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "root": workspace.root(),
                "config_path": workspace.config_path(),
                "model": {
                    "name": config.model.name,
                    "port": config.model.port,
                    "ctx_size": config.model.ctx_size,
                },
                "index": {
                    "max_file_bytes": config.index.max_file_bytes,
                    "extensions": config.index.extensions.len(),
                    "excluded_dirs": config.index.excluded_dirs.len(),
                },
                "engine": {
                    "poll_interval_secs": config.engine.poll_interval_secs,
                },
                "files": discovery.files.len(),
                "metadata": metadata.len(),
                "skipped": discovery.skipped,
                "engine_running": false,
            }))?
        );
        return Ok(());
    }

    println!("Workspace:   {}", workspace.root().display());
    println!("Config:      {}", workspace.config_path().display());
    println!(
        "Modelo:      {} (porta {})",
        config.model.name, config.model.port
    );
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
    println!("Engine:      parado (indexacao chega em c2-e5)");
    Ok(())
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
    let config: Config = workspace.load_config()?;
    if json_output {
        println!("{}", config.to_json()?);
    } else {
        for (key, value) in [
            ("model.name", config.get("model.name")),
            ("model.path", config.get("model.path")),
            ("model.port", config.get("model.port")),
            ("model.ctx_size", config.get("model.ctx_size")),
            ("model.temperature", config.get("model.temperature")),
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
        if !report.failed.is_empty() {
            for path in &report.failed {
                eprintln!("  falhou: {path}");
            }
        }
    }
    Ok(())
}
