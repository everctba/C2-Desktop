//! API key do provedor de nuvem: nunca vai para o `.c2/config.json`.
//!
//! Ordem de resolucao:
//! 1. variavel de ambiente (ex.: `C2_API_KEY`) — boa para CI/headless;
//! 2. cofre do sistema, por plataforma:
//!    - Windows: Credential Manager;
//!    - macOS: Keychain;
//!    - Linux: Secret Service (GNOME Keyring/KWallet).
//!
//! Em sistema sem cofre (CI, servidor headless), use a variavel de ambiente.

use crate::error::{C2Error, Result};

const SERVICO: &str = "c2";
const CONTA: &str = "api-key";

/// Nome do cofre nativo da plataforma (para UI, logs e mensagens de erro).
#[cfg(windows)]
pub const COFRE_NOME: &str = "Windows Credential Manager";
#[cfg(target_os = "macos")]
pub const COFRE_NOME: &str = "macOS Keychain";
#[cfg(target_os = "linux")]
pub const COFRE_NOME: &str = "Secret Service (GNOME Keyring/KWallet)";
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub const COFRE_NOME: &str = "cofre do sistema";

/// Verdadeiro quando o cofre nativo esta disponivel nesta build.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const TEM_COFRE: bool = true;
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
const TEM_COFRE: bool = false;

/// Nome padrao da variavel de ambiente.
pub fn default_env_name() -> &'static str {
    "C2_API_KEY"
}

/// Nome do cofre nativo da plataforma.
pub fn cofre_nome() -> &'static str {
    COFRE_NOME
}

/// Le a chave do ambiente; ignora string vazia.
pub fn from_env(nome: &str) -> Option<String> {
    std::env::var(nome)
        .ok()
        .map(|valor| valor.trim().to_string())
        .filter(|valor| !valor.is_empty())
}

/// Resolve a chave (ambiente primeiro, depois cofre).
pub fn load(nome_env: &str) -> Result<Option<String>> {
    if let Some(chave) = from_env(nome_env) {
        return Ok(Some(chave));
    }
    Ok(carregar_cofre())
}

/// Grava a chave no cofre do sistema.
pub fn save(chave: &str) -> Result<()> {
    let chave = chave.trim();
    if chave.is_empty() {
        return Err(C2Error::InvalidConfig("API key vazia".to_string()));
    }
    salvar_cofre(chave)
}

/// Remove a chave do cofre.
pub fn delete() -> Result<()> {
    remover_cofre();
    Ok(())
}

/// Verdadeiro se existe chave no cofre (nao considera env).
pub fn stored_in_keyring() -> bool {
    carregar_cofre().is_some()
}

// ---------------- backend nativo ----------------

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn entrada() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICO, CONTA)
        .map_err(|err| C2Error::InvalidConfig(format!("cofre indisponivel: {err}")))
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn carregar_cofre() -> Option<String> {
    let entrada = entrada().ok()?;
    entrada
        .get_password()
        .ok()
        .map(|chave| chave.trim().to_string())
        .filter(|chave| !chave.is_empty())
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn salvar_cofre(chave: &str) -> Result<()> {
    entrada()?.set_password(chave).map_err(|err| {
        C2Error::InvalidConfig(format!(
            "nao foi possivel salvar a chave no {}: {err} (alternativa: variavel de ambiente {})",
            COFRE_NOME,
            default_env_name()
        ))
    })
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn remover_cofre() {
    if let Ok(entrada) = keyring::Entry::new(SERVICO, CONTA) {
        let _ = entrada.delete_credential();
    }
}

// ---------------- plataformas sem cofre nativo ----------------

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn carregar_cofre() -> Option<String> {
    None
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn salvar_cofre(_chave: &str) -> Result<()> {
    Err(C2Error::InvalidConfig(format!(
        "cofre de credenciais indisponivel neste sistema; use a variavel de ambiente {}",
        default_env_name()
    )))
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn remover_cofre() {}

/// Diagnostico interno: esta build tem backend de cofre?
pub fn tem_cofre() -> bool {
    TEM_COFRE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_do_cofre_nao_e_vazio() {
        assert!(!COFRE_NOME.trim().is_empty());
        assert!(tem_cofre() == TEM_COFRE);
    }

    #[test]
    fn ambiente_tem_prioridade_sobre_o_cofre() {
        std::env::set_var("C2_TEST_KEY_ENV", "sk-do-ambiente");
        assert_eq!(
            load("C2_TEST_KEY_ENV").unwrap().as_deref(),
            Some("sk-do-ambiente")
        );
        std::env::remove_var("C2_TEST_KEY_ENV");
    }

    #[test]
    fn chave_vazia_e_rejeitada() {
        assert!(save("   ").is_err());
    }
}
