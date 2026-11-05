//! Erros e contexto de IO do nucleo.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum C2Error {
    #[error("workspace C2 nao inicializado em {0} (rode `c2 init`)")]
    NotInitialized(PathBuf),

    #[error("caminho fora do workspace: {0}")]
    OutsideWorkspace(PathBuf),

    #[error("configuracao invalida: {0}")]
    InvalidConfig(String),

    #[error("metadado invalido em {path}: {source}")]
    InvalidMeta {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("falha no modelo: {0}")]
    Llm(String),

    #[error("json invalido: {0}")]
    Json(#[from] serde_json::Error),

    #[error("falha de IO em {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, C2Error>;

impl C2Error {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        C2Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Anexa o caminho ao erro de IO, para que a mensagem diga onde falhou.
pub trait IoContext<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|source| C2Error::io(path, source))
    }
}
