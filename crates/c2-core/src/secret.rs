//! API key do provedor de nuvem: nunca vai para o `.c2/config.json`.
//!
//! Ordem de resolucao:
//! 1. variavel de ambiente (ex.: `C2_API_KEY`) — boa para CI;
//! 2. cofre do sistema (Windows Credential Manager) — para uso normal.

use crate::error::{C2Error, Result};

const SERVICO: &str = "c2";
const CONTA: &str = "api-key";

/// Nome padrao da variavel de ambiente.
pub fn default_env_name() -> &'static str {
    "C2_API_KEY"
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
    #[cfg(windows)]
    {
        let entrada = match keyring::Entry::new(SERVICO, CONTA) {
            Ok(entrada) => entrada,
            Err(_) => return Ok(None),
        };
        match entrada.get_password() {
            Ok(chave) if !chave.trim().is_empty() => Ok(Some(chave)),
            _ => Ok(None),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = nome_env;
        Ok(None)
    }
}

/// Grava a chave no cofre do sistema.
pub fn save(chave: &str) -> Result<()> {
    let chave = chave.trim();
    if chave.is_empty() {
        return Err(C2Error::InvalidConfig("API key vazia".to_string()));
    }
    #[cfg(windows)]
    {
        let entrada = keyring::Entry::new(SERVICO, CONTA)
            .map_err(|err| C2Error::InvalidConfig(format!("cofre indisponivel: {err}")))?;
        entrada.set_password(chave).map_err(|err| {
            C2Error::InvalidConfig(format!("nao foi possivel salvar a chave: {err}"))
        })?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = chave;
        Err(C2Error::InvalidConfig(
            "cofre de credenciais indisponivel neste sistema; use a variavel de ambiente"
                .to_string(),
        ))
    }
}

/// Remove a chave do cofre.
pub fn delete() -> Result<()> {
    #[cfg(windows)]
    {
        if let Ok(entrada) = keyring::Entry::new(SERVICO, CONTA) {
            let _ = entrada.delete_credential();
        }
    }
    Ok(())
}

/// Verdadeiro se existe chave no cofre (nao considera env).
pub fn stored_in_keyring() -> bool {
    #[cfg(windows)]
    {
        if let Ok(entrada) = keyring::Entry::new(SERVICO, CONTA) {
            return entrada
                .get_password()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        }
    }
    false
}
