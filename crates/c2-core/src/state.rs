//! Estado do engine em `.c2/state.json`.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};

/// Contadores acumulados do engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineState {
    /// Ultima varredura (segundos desde a epoch).
    pub last_scan: i64,
    pub indexed: u64,
    pub unchanged: u64,
    pub removed: u64,
    pub errors: u64,
    pub llm_calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub latency_ms: u64,
    pub last_error: Option<String>,
}

impl EngineState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).at(path)?;
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| {
            crate::error::C2Error::InvalidConfig(format!("caminho invalido: {path:?}"))
        })?;
        fs::create_dir_all(parent).at(parent)?;
        let json = serde_json::to_string_pretty(self)?;
        let temp = parent.join(format!(".c2-state-tmp-{}", std::process::id()));
        {
            let mut file = fs::File::create(&temp).at(&temp)?;
            file.write_all(json.as_bytes()).at(&temp)?;
            file.write_all(b"\n").at(&temp)?;
            file.sync_all().at(&temp)?;
        }
        if let Err(err) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(crate::error::C2Error::io(path, err));
        }
        Ok(())
    }

    /// Soma os contadores de um ciclo do indexador.
    pub fn merge_report(&mut self, report: &crate::indexer::IndexReport) {
        self.last_scan = now_secs();
        self.indexed += report.indexed as u64;
        self.unchanged += report.unchanged as u64;
        self.removed += report.removed as u64;
        self.errors += report.errors as u64;
        self.llm_calls += report.calls as u64;
        self.prompt_tokens += report.prompt_tokens;
        self.completion_tokens += report.completion_tokens;
        self.latency_ms += report.latency_ms;
        if let Some(err) = report.last_error.clone() {
            self.last_error = Some(err);
        }
    }
}

/// Segundos desde a epoch.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("c2-state-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn roundtrip_no_disco() {
        let base = tmp("rt");
        let path = base.join("state.json");
        assert_eq!(EngineState::load(&path).unwrap(), EngineState::default());

        let state = EngineState {
            indexed: 7,
            llm_calls: 7,
            latency_ms: 1234,
            ..Default::default()
        };
        state.save(&path).unwrap();
        assert_eq!(EngineState::load(&path).unwrap(), state);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn json_corrompido_volta_ao_default() {
        let base = tmp("corrupt");
        let path = base.join("state.json");
        fs::write(&path, b"{ quebrado").unwrap();
        assert_eq!(EngineState::load(&path).unwrap(), EngineState::default());
        fs::remove_dir_all(&base).ok();
    }
}
