//! Resumo do workspace para interfaces (CLI, desktop, futuros clientes).
//!
//! Fica no Core para que CLI e desktop nao dupliquem a coleta: a interface so
//! formata. Ver `docs/adr/0001-arquitetura-em-camadas.md`.

use serde::Serialize;

use crate::cleaner;
use crate::config::Config;
use crate::discover;
use crate::error::Result;
use crate::runtime;
use crate::state::EngineState;
use crate::workspace::Workspace;

/// Modelo efetivo.
#[derive(Debug, Clone, Serialize)]
pub struct ModelSummary {
    pub name: String,
    pub path: Option<String>,
    pub host: String,
    pub port: u16,
    pub ctx_size: u32,
    pub disable_thinking: bool,
    /// `local` ou `openai_compat`.
    pub provider: String,
    /// Base URL quando o provedor e nuvem/compat.
    pub base_url: Option<String>,
    /// Verdadeiro quando a API key esta disponivel (cofre ou env).
    pub api_key: bool,
}

/// Configuracao de indice efetiva.
#[derive(Debug, Clone, Serialize)]
pub struct IndexSummary {
    pub max_file_bytes: u64,
    pub extensions: usize,
    pub secret_policy: String,
    pub max_total_bytes_per_run: u64,
}

/// Estado do runtime de inferencia.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSummary {
    pub healthy: bool,
    pub pid: Option<u32>,
    pub binary: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    pub url: String,
    /// Ultima mensagem de erro do llama-server, quando houver.
    pub last_error: Option<String>,
}

/// Parametros do laco.
#[derive(Debug, Clone, Serialize)]
pub struct EngineSummary {
    pub poll_interval_secs: u64,
}

/// Tudo o que uma interface precisa para pintar a tela inicial.
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub root: String,
    pub initialized: bool,
    pub config_path: String,
    pub model: ModelSummary,
    pub index: IndexSummary,
    pub engine: EngineSummary,
    pub runtime: RuntimeSummary,
    pub files: usize,
    pub metadata: usize,
    pub skipped: discover::Skipped,
    pub state: EngineState,
}

/// Tempos das fases do status (diagnostico de cold start).
#[derive(Debug, Clone, Default, Serialize)]
pub struct StatusTimings {
    pub config_ms: u64,
    pub discover_ms: u64,
    pub metadata_ms: u64,
    pub state_ms: u64,
    pub runtime_ms: u64,
    pub total_ms: u64,
}

/// Coleta o estado do workspace. Exige workspace inicializado.
pub fn build(workspace: &Workspace) -> Result<StatusReport> {
    build_timed(workspace).map(|(report, _)| report)
}

/// Igual a `build`, mas devolve os tempos de cada fase.
pub fn build_timed(workspace: &Workspace) -> Result<(StatusReport, StatusTimings)> {
    let total = std::time::Instant::now();
    let mut timings = StatusTimings::default();

    let inicio = std::time::Instant::now();
    let config = workspace.load_config()?;
    timings.config_ms = inicio.elapsed().as_millis() as u64;

    let inicio = std::time::Instant::now();
    let discovery = discover::discover(workspace.root(), &config)?;
    timings.discover_ms = inicio.elapsed().as_millis() as u64;

    let inicio = std::time::Instant::now();
    let metadata = cleaner::metadata_files(workspace.root())?;
    timings.metadata_ms = inicio.elapsed().as_millis() as u64;

    let inicio = std::time::Instant::now();
    let state = EngineState::load(&workspace.state_path())?;
    timings.state_ms = inicio.elapsed().as_millis() as u64;

    let inicio = std::time::Instant::now();
    let runtime_status = runtime::status(workspace, &config);
    timings.runtime_ms = inicio.elapsed().as_millis() as u64;
    timings.total_ms = total.elapsed().as_millis() as u64;

    let report = StatusReport {
        root: workspace.root().display().to_string(),
        initialized: workspace.is_initialized(),
        config_path: workspace.config_path().display().to_string(),
        model: ModelSummary {
            name: config.model.name.clone(),
            provider: config.model.provider.label().to_string(),
            base_url: config.model.base_url.clone(),
            api_key: crate::secret::load(&config.model.api_key_env)
                .ok()
                .flatten()
                .is_some(),
            path: config.model.path.clone(),
            host: config.model.host.clone(),
            port: config.model.port,
            ctx_size: config.model.ctx_size,
            disable_thinking: config.model.disable_thinking,
        },
        index: IndexSummary {
            max_file_bytes: config.index.max_file_bytes,
            extensions: config.index.extensions.len(),
            secret_policy: format!("{:?}", config.index.secret_policy).to_lowercase(),
            max_total_bytes_per_run: config.index.max_total_bytes_per_run,
        },
        engine: EngineSummary {
            poll_interval_secs: config.engine.poll_interval_secs,
        },
        runtime: RuntimeSummary {
            healthy: runtime_status.healthy,
            pid: runtime_status.pid,
            binary: runtime_status.binary.map(|path| path.display().to_string()),
            source: runtime_status.source,
            version: runtime_status.version,
            url: config.model.health_url(),
            last_error: crate::runtime::last_error(workspace),
        },
        files: discovery.files.len(),
        metadata: metadata.len(),
        skipped: discovery.skipped,
        state,
    };
    Ok((report, timings))
}

/// Configuracao efetiva, para telas de ajustes.
pub fn current_config(workspace: &Workspace) -> Result<Config> {
    workspace.load_config()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("c2-status-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resume_arquivos_e_metadados() {
        let base = tmp("basico");
        fs::write(base.join("a.rs"), "fn main() {}").unwrap();
        fs::write(base.join("b.md"), "# doc").unwrap();
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();

        let report = build(&workspace).unwrap();
        assert!(report.initialized);
        assert_eq!(report.files, 2);
        assert_eq!(report.metadata, 0);
        assert_eq!(report.model.port, 8080);
        assert!(!report.runtime.healthy);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn conta_metadados_existentes() {
        let base = tmp("meta");
        fs::write(base.join("a.rs"), "fn main() {}").unwrap();
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        fs::write(base.join("a.rs.c2.json"), "{}").unwrap();

        let report = build(&workspace).unwrap();
        assert_eq!(report.metadata, 1);

        fs::remove_dir_all(&base).ok();
    }
}
