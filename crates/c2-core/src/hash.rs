//! SHA-256 do conteudo, em streaming.
//!
//! O hash sempre inclui o prefixo `sha256:` para deixar explicito o algoritmo
//! no proprio metadado.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

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

/// Entrada do cache de hash: impressao digital barata do arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    pub mtime: i64,
    pub hash: String,
}

/// Cache de rehash por (tamanho, mtime) — bead `c2-e3t1`.
///
/// Se tamanho e mtime batem, o hash anterior e reaproveitado sem abrir o
/// arquivo. E uma otimizacao: `--full` ignora o cache e recalcula tudo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashCache {
    #[serde(default = "versao_padrao")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: HashMap<String, CacheEntry>,
}

fn versao_padrao() -> u32 {
    1
}

impl Default for HashCache {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: HashMap::new(),
        }
    }
}

impl HashCache {
    /// Carrega o cache; arquivo ausente/corrompido vira cache vazio.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(crate::error::C2Error::from)?;
        if let Some(pai) = path.parent() {
            fs::create_dir_all(pai).at(pai)?;
        }
        fs::write(path, bytes).at(path)
    }

    /// Hash em cache, se tamanho e mtime conferem.
    pub fn get(&self, rel: &str, size: u64, mtime: i64) -> Option<String> {
        let entry = self.entries.get(rel)?;
        if entry.size == size && entry.mtime == mtime {
            Some(entry.hash.clone())
        } else {
            None
        }
    }

    pub fn insert(&mut self, rel: &str, size: u64, mtime: i64, hash: String) {
        self.entries
            .insert(rel.to_string(), CacheEntry { size, mtime, hash });
    }

    /// Substitui todas as entradas (descarta arquivos que sumiram).
    pub fn replace(&mut self, entries: HashMap<String, CacheEntry>) {
        self.entries = entries;
    }

    /// Remove entradas que nao estao no conjunto atual.
    pub fn prune(&mut self, manter: &BTreeSet<String>) {
        self.entries.retain(|rel, _| manter.contains(rel));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("c2-hashcache-{tag}-{stamp}.json"))
    }

    #[test]
    fn guarda_e_reaproveita_quando_tamanho_e_mtime_batem() {
        let path = tmp("hit");
        let mut cache = HashCache::default();
        cache.insert("src/a.rs", 100, 1700000000, "sha256:abc".to_string());
        cache.save(&path).unwrap();

        let carregado = HashCache::load(&path);
        assert_eq!(
            carregado.get("src/a.rs", 100, 1700000000),
            Some("sha256:abc".to_string())
        );
        assert_eq!(carregado.get("src/a.rs", 101, 1700000000), None);
        assert_eq!(carregado.get("src/a.rs", 100, 1700000001), None);
        assert_eq!(carregado.get("src/outro.rs", 100, 1700000000), None);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn arquivo_corrompido_vira_cache_vazio() {
        let path = tmp("corrupt");
        std::fs::write(&path, b"nao e json").unwrap();
        let cache = HashCache::load(&path);
        assert!(cache.entries.is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn prune_remove_entradas_antigas() {
        let mut cache = HashCache::default();
        cache.insert("a.rs", 1, 1, "h".to_string());
        cache.insert("b.rs", 1, 1, "h".to_string());
        let manter: BTreeSet<String> = ["a.rs".to_string()].into_iter().collect();
        cache.prune(&manter);
        assert!(cache.entries.contains_key("a.rs"));
        assert!(!cache.entries.contains_key("b.rs"));
    }
}
