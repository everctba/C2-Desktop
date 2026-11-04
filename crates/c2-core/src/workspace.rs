//! Workspace do C2: descoberta da raiz, layout de `.c2/` e inicializacao.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::Config;
use crate::discover;
use crate::error::{C2Error, IoContext, Result};

/// Diretorio de estado do workspace.
pub const C2_DIR: &str = ".c2";
/// Configuracao do workspace.
pub const CONFIG_FILE: &str = "config.json";
/// Estado do engine.
pub const STATE_FILE: &str = "state.json";
/// Diretorio de logs.
pub const LOGS_DIR: &str = "logs";

/// Raiz de um projeto indexado pelo C2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

/// Resultado da inicializacao.
#[derive(Debug, Clone, Serialize)]
pub struct InitOutcome {
    /// `false` quando o workspace ja existia e nada foi sobrescrito.
    pub created: bool,
    pub root: PathBuf,
    pub config_path: PathBuf,
    /// Arquivos descobertos com a configuracao efetiva.
    pub files: usize,
    /// Contagem por extensao (top 10).
    pub languages: BTreeMap<String, usize>,
}

/// Canonicaliza e remove o prefixo verbatim do Windows (`\\?\`), que so
/// aparece para a API mas polui a saida da CLI.
pub fn canonical_path(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path).at(path)?;
    Ok(strip_verbatim(canonical))
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().to_string();
        if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
            return PathBuf::from(format!("\\\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix("\\\\?\\") {
            return PathBuf::from(rest);
        }
    }
    path
}

impl Workspace {
    /// Cria um workspace a partir de uma raiz conhecida.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Raiz informada.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn c2_dir(&self) -> PathBuf {
        self.root.join(C2_DIR)
    }

    pub fn config_path(&self) -> PathBuf {
        self.c2_dir().join(CONFIG_FILE)
    }

    pub fn state_path(&self) -> PathBuf {
        self.c2_dir().join(STATE_FILE)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.c2_dir().join(LOGS_DIR)
    }

    /// Verdadeiro quando `.c2/config.json` existe.
    pub fn is_initialized(&self) -> bool {
        self.config_path().is_file()
    }

    /// Sobe a partir de `start` ate achar um workspace inicializado.
    pub fn find(start: &Path) -> Result<Option<Self>> {
        let mut current = canonical_path(start)?;
        if current.is_file() {
            current.pop();
        }
        for _ in 0..64 {
            let candidate = Self::at(current.clone());
            if candidate.is_initialized() {
                return Ok(Some(candidate));
            }
            if !current.pop() {
                break;
            }
        }
        Ok(None)
    }

    /// Inicializa o workspace. Idempotente: com a config existente e sem
    /// `force`, nada e sobrescrito.
    pub fn init(&self, force: bool) -> Result<InitOutcome> {
        let c2_dir = self.c2_dir();
        fs::create_dir_all(&c2_dir).at(&c2_dir)?;

        let logs = self.logs_dir();
        fs::create_dir_all(&logs).at(&logs)?;

        let config_path = self.config_path();
        let created = !config_path.exists() || force;
        if created {
            Config::default().write_atomic(&config_path)?;
        }

        let config = self.load_config()?;
        let discovery = discover::discover(&self.root, &config)?;

        let mut languages: BTreeMap<String, usize> = BTreeMap::new();
        for entry in &discovery.files {
            *languages.entry(entry.extension()).or_insert(0) += 1;
        }

        Ok(InitOutcome {
            created,
            root: self.root.clone(),
            config_path,
            files: discovery.files.len(),
            languages,
        })
    }

    /// Carrega a configuracao efetiva.
    pub fn load_config(&self) -> Result<Config> {
        let path = self.config_path();
        if !path.is_file() {
            return Err(C2Error::NotInitialized(self.root.clone()));
        }
        Config::read(&path)
    }

    /// Grava a configuracao efetiva.
    pub fn save_config(&self, config: &Config) -> Result<()> {
        config.write_atomic(&self.config_path())
    }

    /// Canoniciza `path` e exige que ele esteja dentro da raiz do workspace.
    pub fn ensure_inside(&self, path: &Path) -> Result<PathBuf> {
        let root = canonical_path(&self.root)?;
        let target = canonical_path(path)?;
        if !target.starts_with(&root) {
            return Err(C2Error::OutsideWorkspace(target));
        }
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("c2-ws-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn init_cria_layout_e_e_idempotente() {
        let base = tmp("init");
        fs::write(base.join("a.rs"), "fn main() {}").unwrap();
        let ws = Workspace::at(&base);

        assert!(!ws.is_initialized());
        let first = ws.init(false).unwrap();
        assert!(first.created);
        assert!(ws.config_path().is_file());
        assert!(ws.logs_dir().is_dir());
        assert_eq!(first.files, 1);

        let second = ws.init(false).unwrap();
        assert!(!second.created, "nao deve sobrescrever config existente");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn find_sobe_ate_a_raiz() {
        let base = tmp("find");
        let ws = Workspace::at(&base);
        ws.init(false).unwrap();
        let nested = base.join("src").join("deep");
        fs::create_dir_all(&nested).unwrap();

        let found = Workspace::find(&nested).unwrap().expect("workspace");
        assert_eq!(
            fs::canonicalize(found.root()).unwrap(),
            fs::canonicalize(&base).unwrap()
        );

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn config_ausente_vira_erro_de_nao_inicializado() {
        let base = tmp("notinit");
        let err = Workspace::at(&base).load_config().unwrap_err();
        assert!(matches!(err, C2Error::NotInitialized(_)));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ensure_inside_recusa_caminho_de_fora() {
        let base = tmp("inside");
        let ws = Workspace::at(&base);
        ws.init(false).unwrap();
        let inside = base.join("a.rs");
        fs::write(&inside, "x").unwrap();
        assert!(ws.ensure_inside(&inside).is_ok());
        assert!(ws.ensure_inside(std::env::temp_dir().as_path()).is_err());
        fs::remove_dir_all(&base).ok();
    }
}
