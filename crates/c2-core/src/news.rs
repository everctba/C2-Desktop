//! Busca do `news.json` estatico para o popup de novidades ("What's New").
//!
//! O conteudo remoto e untrusted: aqui so baixamos bytes com timeout e limite
//! de tamanho; validacao/escape acontece na UI.

use std::io::Read;
use std::time::Duration;

use crate::error::{C2Error, Result};

/// Tamanho maximo da resposta (protecao contra payload gigante).
pub const MAX_BYTES: usize = 256 * 1024;
/// Timeout da requisicao.
pub const TIMEOUT_SECS: u64 = 8;

/// Baixa o JSON de novidades de uma URL http/https.
pub fn fetch(url: &str) -> Result<String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(C2Error::InvalidConfig(format!(
            "news: URL invalida (so http/https): {url}"
        )));
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build();
    let resposta = agent
        .get(url)
        .call()
        .map_err(|erro| C2Error::InvalidConfig(format!("news: falha ao buscar {url}: {erro}")))?;

    if resposta.status() != 200 {
        return Err(C2Error::InvalidConfig(format!(
            "news: {url} respondeu {}",
            resposta.status()
        )));
    }

    let mut texto = String::new();
    resposta
        .into_reader()
        .take((MAX_BYTES + 1) as u64)
        .read_to_string(&mut texto)
        .map_err(|erro| C2Error::InvalidConfig(format!("news: erro de leitura: {erro}")))?;

    if texto.len() > MAX_BYTES {
        return Err(C2Error::InvalidConfig(format!(
            "news: resposta maior que {MAX_BYTES} bytes"
        )));
    }
    Ok(texto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recusa_url_nao_http() {
        assert!(fetch("file:///etc/passwd").is_err());
        assert!(fetch("javascript:alert(1)").is_err());
        assert!(fetch("").is_err());
    }
}
