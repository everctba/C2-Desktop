//! Nucleo do C2: descoberta de arquivos, hash e metadados semanticos.
//!
//! Este crate nao conhece CLI, desktop nem MCP: quem consome e a interface.
//! Ver `docs/adr/0001-arquitetura-em-camadas.md`.

pub mod cleaner;
pub mod config;
pub mod discover;
pub mod error;
pub mod hash;
pub mod indexer;
pub mod links;
pub mod llm;
pub mod runtime;
pub mod schema;
pub mod search;
pub mod security;
pub mod state;
pub mod workspace;

pub use config::Config;
pub use error::{C2Error, Result};
pub use indexer::{IndexOptions, IndexReport};
pub use llm::{Generated, LlmClient, SemanticProvider};
pub use schema::{Link, MetaFile, Semantic};
pub use search::{Hit, SearchOptions};
pub use state::EngineState;
pub use workspace::Workspace;

/// Versao do nucleo, usada por `c2 --version`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
