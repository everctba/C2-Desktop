//! Infraestrutura de embeddings locais (bead `c2-kga.2`).
//!
//! - um vetor por arquivo, derivado dos campos semanticos do `.c2.json`;
//! - cache em `.c2/embeddings/<modelo>-<dim>d/<arquivo>.json`;
//! - invalidação por hash do metadado, modelo e dimensão;
//! - cosine e normalização em Rust puro.
//!
//! O runtime que gera os vetores fica fora deste módulo: aqui ficam apenas o
//! contrato, o cache e a matemática.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::schema::MetaFile;
use crate::workspace::Workspace;

/// Distância de cosseno normalizada.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return None;
    }
    Some(dot / (na.sqrt() * nb.sqrt()))
}

/// Normaliza para norma 1 (bge-m3 e afins já devolvem normalizado, mas o cache
/// não depende disso).
pub fn normalize(vector: &[f32]) -> Vec<f32> {
    let norma = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norma <= f32::EPSILON {
        return vector.to_vec();
    }
    vector.iter().map(|v| v / norma).collect()
}

/// Texto enviado ao modelo de embedding. Os pesos são propositalmente simples
/// e determinísticos; o objetivo é priorizar responsabilidade/resumo.
pub fn text_for(meta: &MetaFile) -> String {
    let s = &meta.semantic;
    let mut partes = Vec::new();
    if !s.summary.trim().is_empty() {
        partes.push(format!("resumo: {}", s.summary.trim()));
    }
    if !s.responsibility.trim().is_empty() {
        partes.push(format!("responsabilidade: {}", s.responsibility.trim()));
    }
    if !s.what.trim().is_empty() {
        partes.push(format!("o que e: {}", s.what.trim()));
    }
    if !s.why.trim().is_empty() {
        partes.push(format!("por que existe: {}", s.why.trim()));
    }
    if !s.goal.trim().is_empty() {
        partes.push(format!("objetivo: {}", s.goal.trim()));
    }
    if !meta.links.is_empty() {
        let links = meta
            .links
            .iter()
            .map(|link| link.to.clone())
            .collect::<Vec<_>>()
            .join(", ");
        partes.push(format!("relacionados: {links}"));
    }
    partes.join("\n")
}

/// Registro persistido no cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedEmbedding {
    model_id: String,
    dim: usize,
    source_hash: String,
    vector: Vec<f32>,
}

/// Cache de vetores de um workspace.
#[derive(Debug, Clone)]
pub struct EmbeddingCache {
    root: PathBuf,
    model_id: String,
    dim: usize,
}

impl EmbeddingCache {
    pub fn new(workspace: &Workspace, model_id: &str, dim: usize) -> Self {
        let slug = slug(model_id);
        Self {
            root: workspace
                .c2_dir()
                .join("embeddings")
                .join(format!("{slug}-{dim}d")),
            model_id: model_id.to_string(),
            dim,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Caminho do cache para um arquivo-fonte relativo.
    pub fn path_for(&self, rel: &str) -> Option<PathBuf> {
        let rel = rel.trim_start_matches('/').replace('\\', "/");
        if rel.is_empty()
            || rel
                .split('/')
                .any(|parte| parte.is_empty() || parte == ".." || parte == ".")
            || rel.contains(':')
        {
            return None;
        }
        Some(self.root.join(format!("{rel}.json")))
    }

    /// Carrega o vetor se ele corresponder ao hash/modelo/dimensão atuais.
    pub fn load(&self, rel: &str, source_hash: &str) -> Option<Vec<f32>> {
        let texto = fs::read_to_string(self.path_for(rel)?).ok()?;
        let cache: CachedEmbedding = serde_json::from_str(&texto).ok()?;
        if cache.model_id != self.model_id
            || cache.dim != self.dim
            || cache.vector.len() != self.dim
            || cache.source_hash != source_hash
        {
            return None;
        }
        Some(cache.vector)
    }

    /// Grava de forma atômica (tmp + rename). Vetor vazio/inválido não é aceito.
    pub fn store(&self, rel: &str, source_hash: &str, vector: &[f32]) -> std::io::Result<()> {
        if vector.len() != self.dim || vector.iter().all(|v| !v.is_finite()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "vetor invalido para o cache",
            ));
        }
        let destino = self.path_for(rel).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "caminho invalido")
        })?;
        if let Some(pai) = destino.parent() {
            fs::create_dir_all(pai)?;
        }
        let registro = CachedEmbedding {
            model_id: self.model_id.clone(),
            dim: self.dim,
            source_hash: source_hash.to_string(),
            vector: vector.to_vec(),
        };
        let tmp = destino.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(&registro)?)?;
        fs::rename(tmp, destino)?;
        Ok(())
    }

    /// Remove vetores órfãos de arquivos que não existem mais.
    pub fn prune(&self, existentes: &[String]) -> std::io::Result<usize> {
        if !self.root.is_dir() {
            return Ok(0);
        }
        let mut removidos = 0;
        for entrada in walk(&self.root) {
            let Ok(rel) = entrada.strip_prefix(&self.root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let rel = rel.strip_suffix(".json").unwrap_or(&rel).to_string();
            if !existentes.iter().any(|item| item == &rel) {
                fs::remove_file(&entrada)?;
                removidos += 1;
            }
        }
        Ok(removidos)
    }
}

fn slug(valor: &str) -> String {
    valor
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    let Ok(entradas) = fs::read_dir(dir) else {
        return saida;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            saida.extend(walk(&caminho));
        } else if caminho.is_file() {
            saida.push(caminho);
        }
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Link, Semantic};

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("c2-embedding-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn meta(hash: &str) -> MetaFile {
        MetaFile::new(
            "src/pedido.rs",
            hash,
            10,
            0,
            Semantic {
                what: "valida pagamento".to_string(),
                why: "evita cobrança indevida".to_string(),
                goal: "pedido só avança aprovado".to_string(),
                summary: "consulta o provedor e atualiza o status".to_string(),
                responsibility: "regras de confirmação".to_string(),
            },
            vec![Link {
                to: "src/provedor.rs".to_string(),
                kind: "imports".to_string(),
            }],
        )
    }

    #[test]
    fn cosine_de_vetores_conhecidos() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]).unwrap() - 1.0).abs() < 1e-5);
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0]).unwrap()).abs() < 1e-5);
        assert!(cosine(&[1.0], &[1.0, 0.0]).is_none());
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).is_none());
    }

    #[test]
    fn roundtrip_e_invalidacao_do_cache() {
        let dir = tmp("cache");
        let workspace = Workspace::at(dir.clone());
        let cache = EmbeddingCache::new(&workspace, "bge-m3", 4);
        let vetor = vec![1.0, 2.0, 3.0, 4.0];
        cache.store("src/pedido.rs", "sha256:abc", &vetor).unwrap();
        assert_eq!(cache.load("src/pedido.rs", "sha256:abc"), Some(vetor));
        assert_eq!(cache.load("src/pedido.rs", "sha256:outro"), None);
        assert_eq!(cache.load("src/outro.rs", "sha256:abc"), None);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn texto_prioriza_resumo_e_nao_quebra_sem_campos() {
        let texto = text_for(&meta("sha256:abc"));
        assert!(texto.starts_with("resumo:"));
        assert!(texto.contains("src/provedor.rs"));
        let vazio = MetaFile::new("x.rs", "h", 0, 0, Semantic::default(), Vec::new());
        assert!(text_for(&vazio).is_empty());
    }
}
