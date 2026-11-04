//! SHA-256 do conteudo, em streaming.
//!
//! O hash sempre inclui o prefixo `sha256:` para deixar explicito o algoritmo
//! no proprio metadado.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{IoContext, Result};

const READ_SIZE: usize = 64 * 1024;

/// Hash de um bloco de bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", hex_lower(&digest))
}

/// Hash do arquivo, lendo em blocos (nao carrega o arquivo inteiro).
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).at(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; READ_SIZE];
    loop {
        let read = file.read(&mut buffer).at(path)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex_lower(&hasher.finalize())))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vetor_conhecido() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_do_arquivo_bate_com_o_de_bytes() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("c2-hash-{}-{stamp}.txt", std::process::id()));
        std::fs::write(&path, b"conteudo").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(b"conteudo"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn arquivo_grande_e_lido_em_blocos() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("c2-hash-big-{}-{stamp}.bin", std::process::id()));
        let data = vec![7u8; 200 * 1024];
        std::fs::write(&path, &data).unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(&data));
        std::fs::remove_file(&path).ok();
    }
}
