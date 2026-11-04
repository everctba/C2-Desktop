//! Nucleo do C2: descoberta de arquivos, hash e metadados semanticos.
//!
//! Este crate nao conhece CLI, desktop nem MCP: quem consome e a interface.
//! Ver `docs/adr/0001-arquitetura-em-camadas.md`.

pub mod cleaner;
pub mod config;
pub mod discover;
pub mod error;
pub mod hash;
pub mod schema;
pub mod workspace;

pub use config::Config;
pub use error::{C2Error, Result};
pub use schema::{Link, MetaFile, Semantic};
pub use workspace::Workspace;

/// Versao do nucleo, usada por `c2 --version`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
