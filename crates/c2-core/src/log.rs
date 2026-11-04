//! Log em arquivo com rotacao (bead `c2-e10t1`).
//!
//! Formato JSONL em `.c2/logs/c2.log`: `{"ts":<epoch>,"level":"info",...}`.
//! Quando o arquivo passa de `log.max_file_bytes`, roda `c2.log.1 … c2.log.N`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{C2Error, IoContext, Result};
use crate::state::now_secs;
use crate::workspace::Workspace;

/// Nivel de log (quanto menor, mais critico).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    pub fn parse(texto: &str) -> Self {
        match texto.trim().to_ascii_lowercase().as_str() {
            "error" | "erro" => Level::Error,
            "warn" | "warning" | "aviso" => Level::Warn,
            "debug" => Level::Debug,
            _ => Level::Info,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }
}

/// Escritor de log do workspace.
#[derive(Debug, Clone)]
pub struct Logger {
    dir: PathBuf,
    level: Level,
    max_bytes: u64,
    keep: usize,
}

impl Logger {
    pub fn new(dir: impl Into<PathBuf>, level: Level, max_bytes: u64, keep: usize) -> Self {
        Self {
            dir: dir.into(),
            level,
            max_bytes: max_bytes.max(1024),
            keep: keep.max(1),
        }
    }

    /// Logger a partir do workspace e da configuracao efetiva.
    pub fn for_workspace(workspace: &Workspace, config: &Config) -> Self {
        Self::new(
            workspace.logs_dir(),
            Level::parse(&config.log.level),
            config.log.max_file_bytes,
            config.log.keep,
        )
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join("c2.log")
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn error(&self, module: &str, message: impl AsRef<str>) {
        self.log(Level::Error, module, message);
    }

    pub fn warn(&self, module: &str, message: impl AsRef<str>) {
        self.log(Level::Warn, module, message);
    }

    pub fn info(&self, module: &str, message: impl AsRef<str>) {
        self.log(Level::Info, module, message);
    }

    pub fn debug(&self, module: &str, message: impl AsRef<str>) {
        self.log(Level::Debug, module, message);
    }

    pub fn log(&self, level: Level, module: &str, message: impl AsRef<str>) {
        if level > self.level {
            return;
        }
        let _ = self.write(level, module, message.as_ref());
    }

    fn write(&self, level: Level, module: &str, message: &str) -> Result<()> {
        fs::create_dir_all(&self.dir).at(&self.dir)?;
        let path = self.path();
        self.rotate_if_needed(&path)?;

        let linha = serde_json::json!({
            "ts": now_secs(),
            "level": level.label(),
            "module": module,
            "message": message,
        });
        let mut arquivo = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .at(&path)?;
        writeln!(arquivo, "{linha}").at(&path)?;
        Ok(())
    }

    fn rotate_if_needed(&self, path: &Path) -> Result<()> {
        let Ok(meta) = fs::metadata(path) else {
            return Ok(());
        };
        if meta.len() < self.max_bytes {
            return Ok(());
        }

        // c2.log.(keep-1) -> c2.log.keep ... c2.log -> c2.log.1
        let penultimo = self.dir.join(format!("c2.log.{}", self.keep));
        if penultimo.exists() {
            fs::remove_file(&penultimo).at(&penultimo)?;
        }
        for indice in (1..self.keep).rev() {
            let origem = self.dir.join(format!("c2.log.{indice}"));
            let destino = self.dir.join(format!("c2.log.{}", indice + 1));
            if origem.exists() {
                let _ = fs::rename(&origem, &destino);
            }
        }
        let primeiro = self.dir.join("c2.log.1");
        fs::rename(path, &primeiro).at(&primeiro)?;
        Ok(())
    }

    /// Ultimas `linhas` linhas do log (mais antigas primeiro).
    pub fn tail(&self, linhas: usize) -> Vec<String> {
        let path = self.path();
        let Ok(conteudo) = fs::read_to_string(&path) else {
            return Vec::new();
        };
        let mut ultimas: Vec<String> = conteudo
            .lines()
            .rev()
            .take(linhas)
            .map(str::to_string)
            .collect();
        ultimas.reverse();
        ultimas
    }

    /// Tamanho atual do log em bytes.
    pub fn size_bytes(&self) -> u64 {
        fs::metadata(self.path())
            .map(|meta| meta.len())
            .unwrap_or(0)
    }
}

/// Valida os parametros de log da configuracao.
pub fn validate(config: &Config) -> Result<()> {
    if config.log.keep == 0 {
        return Err(C2Error::InvalidConfig("log.keep deve ser >= 1".to_string()));
    }
    if config.log.max_file_bytes < 1024 {
        return Err(C2Error::InvalidConfig(
            "log.max_file_bytes deve ser >= 1024".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("c2-log-{tag}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn escreve_jsonl_com_nivel_e_modulo() {
        let dir = tmp("write");
        let logger = Logger::new(&dir, Level::Debug, 10_000, 3);
        logger.info("indexer", "arquivo indexado");
        logger.error("runtime", "servidor caiu");

        let conteudo = fs::read_to_string(logger.path()).unwrap();
        let linhas: Vec<&str> = conteudo.lines().collect();
        assert_eq!(linhas.len(), 2);
        let primeiro: serde_json::Value = serde_json::from_str(linhas[0]).unwrap();
        assert_eq!(primeiro["level"], "info");
        assert_eq!(primeiro["module"], "indexer");
        assert_eq!(primeiro["message"], "arquivo indexado");
        assert!(primeiro["ts"].as_i64().unwrap() > 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respeita_nivel_minimo() {
        let dir = tmp("level");
        let logger = Logger::new(&dir, Level::Warn, 10_000, 2);
        logger.debug("x", "nao deve aparecer");
        logger.info("x", "tambem nao");
        logger.warn("x", "aparece");
        let conteudo = fs::read_to_string(logger.path()).unwrap();
        assert_eq!(conteudo.lines().count(), 1);
        assert!(conteudo.contains("aparece"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotaciona_ao_atingir_o_limite() {
        let dir = tmp("rotate");
        let logger = Logger::new(&dir, Level::Debug, 1024, 3);
        for indice in 0..80 {
            logger.info("teste", format!("linha {indice} {}", "x".repeat(40)));
        }
        assert!(logger.path().exists());
        assert!(dir.join("c2.log.1").exists(), "deve ter rotacionado");
        let total: u64 = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("c2.log"))
            .map(|entry| entry.metadata().map(|meta| meta.len()).unwrap_or(0))
            .sum();
        assert!(total > 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tail_devolve_ultimas_linhas() {
        let dir = tmp("tail");
        let logger = Logger::new(&dir, Level::Debug, 10_000, 2);
        for indice in 0..5 {
            logger.info("x", format!("linha {indice}"));
        }
        let ultimas = logger.tail(2);
        assert_eq!(ultimas.len(), 2);
        assert!(ultimas[1].contains("linha 4"));
        fs::remove_dir_all(&dir).ok();
    }
}
