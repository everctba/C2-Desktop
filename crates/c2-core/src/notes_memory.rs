//! Memoria de engenharia: busca lexical/embeddings sobre `agent_notes` (c2-kf0).

use std::collections::BTreeSet;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::embedding::{self, EmbeddingCache};
use crate::embedding_runtime;
use crate::error::{C2Error, Result};
use crate::schema::AgentNote;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize)]
pub struct NoteHit {
    pub file: String,
    pub kind: String,
    pub text: String,
    pub task: Option<String>,
    pub by: String,
    pub at: String,
    pub score: f32,
    pub cosine: Option<f32>,
}

fn hash_nota(file: &str, nota: &AgentNote) -> String {
    let mut h = Sha256::new();
    h.update(file.as_bytes());
    h.update(b"\0");
    h.update(nota.at.as_bytes());
    h.update(b"\0");
    h.update(nota.kind.as_bytes());
    h.update(b"\0");
    h.update(nota.text.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

fn texto_nota(file: &str, nota: &AgentNote) -> String {
    let mut partes = vec![
        format!("arquivo: {file}"),
        format!("tipo: {}", nota.kind),
        nota.text.clone(),
    ];
    if let Some(task) = &nota.task {
        partes.push(format!("tarefa: {task}"));
    }
    partes.join("\n")
}

fn termos(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_string())
        .collect()
}

fn lexical(file: &str, nota: &AgentNote, consulta: &[String]) -> f32 {
    let texto = format!(
        "{} {} {} {}",
        file,
        nota.kind,
        nota.text,
        nota.task.clone().unwrap_or_default()
    )
    .to_lowercase();
    let mut score = 0.0;
    for termo in consulta {
        if texto.contains(termo) {
            score += 1.0;
            if nota.text.to_lowercase().contains(termo) {
                score += 1.0;
            }
        }
    }
    score
}

fn notas(workspace: &Workspace) -> Result<Vec<(String, AgentNote)>> {
    let mut saida = Vec::new();
    for (file, lista) in crate::notes::list(workspace, None)? {
        for nota in lista {
            saida.push((file.clone(), nota));
        }
    }
    Ok(saida)
}

/// Gera/atualiza vetores das notas. Requer runtime de embeddings saudavel.
pub fn index(workspace: &Workspace, config: &Config) -> Result<usize> {
    let cfg = &config.index.embeddings;
    if !cfg.enabled {
        return Ok(0);
    }
    let _ = embedding_runtime::ensure(workspace, config)?;
    let cache = EmbeddingCache::new(workspace, &cfg.model_id, cfg.dim);
    let mut gerados = 0usize;
    for (file, nota) in notas(workspace)? {
        let hash = hash_nota(&file, &nota);
        let chave = format!("notes/{file}-{}", hash.trim_start_matches("sha256:"));
        if cache.load(&chave, &hash).is_some() {
            continue;
        }
        let vetor = embedding_runtime::embed(config, &texto_nota(&file, &nota))?;
        if vetor.len() == cfg.dim {
            cache
                .store(&chave, &hash, &vetor)
                .map_err(|erro| C2Error::InvalidConfig(format!("cache de nota: {erro}")))?;
            gerados += 1;
        }
    }
    Ok(gerados)
}

/// Busca notas. Sem embeddings configurados, usa apenas lexical.
pub fn search(
    workspace: &Workspace,
    config: &Config,
    query: &str,
    limite: usize,
) -> Result<Vec<NoteHit>> {
    let consulta = termos(query);
    let cfg = &config.index.embeddings;
    let cache = EmbeddingCache::new(workspace, &cfg.model_id, cfg.dim);
    let query_vetor = if cfg.enabled {
        embedding_runtime::embed(config, query).ok()
    } else {
        None
    };
    let mut hits = Vec::new();
    for (file, nota) in notas(workspace)? {
        let lex = lexical(&file, &nota, &consulta);
        let hash = hash_nota(&file, &nota);
        let chave = format!("notes/{file}-{}", hash.trim_start_matches("sha256:"));
        let cosine = match (&query_vetor, cache.load(&chave, &hash)) {
            (Some(q), Some(v)) => embedding::cosine(q, &v),
            _ => None,
        };
        let sem_score = cosine.unwrap_or(0.0);
        if !consulta.is_empty() && lex <= 0.0 && sem_score < cfg.min_score {
            continue;
        }
        let score = lex + cfg.weight * sem_score;
        hits.push((
            score,
            NoteHit {
                file,
                kind: nota.kind,
                text: nota.text,
                task: nota.task,
                by: nota.by,
                at: nota.at,
                score,
                cosine,
            },
        ));
    }
    hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut saida: Vec<NoteHit> = hits.into_iter().map(|(_, hit)| hit).collect();
    saida.truncate(limite);
    Ok(saida)
}

/// Contagem por kind (para pagina de memoria).
pub fn por_kind(workspace: &Workspace) -> Result<Vec<(String, usize)>> {
    let mut contagem: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut vistos = BTreeSet::new();
    for (file, nota) in notas(workspace)? {
        let chave = format!("{}:{}:{}", file, nota.at, nota.text);
        if vistos.insert(chave) {
            *contagem.entry(nota.kind).or_default() += 1;
        }
    }
    Ok(contagem.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{MetaFile, Semantic};

    fn demo(tag: &str) -> (std::path::PathBuf, Workspace) {
        let dir = std::env::temp_dir().join(format!(
            "c2-notes-memory-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join(".c2")).unwrap();
        let workspace = Workspace::at(dir.clone());
        workspace.init(false).unwrap();
        let mut meta = MetaFile::new(
            "src/pagamento.rs",
            "sha256:a",
            1,
            0,
            Semantic {
                what: "pagamento".to_string(),
                why: "cobrar".to_string(),
                goal: "confirmar".to_string(),
                summary: "consulta provedor".to_string(),
                responsibility: "retry".to_string(),
            },
            vec![],
        );
        meta.agent_notes.push(AgentNote {
            by: "agent".to_string(),
            model: None,
            task: Some("retry".to_string()),
            at: "2026-11-10T10:00:00Z".to_string(),
            hash: Some("sha256:a".to_string()),
            kind: "gotcha".to_string(),
            text: "O retry nao pode ser removido: o provedor devolve 202 antes de confirmar."
                .to_string(),
        });
        let path = crate::schema::meta_path_for(&dir.join("src/pagamento.rs"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        meta.write_atomic(&path).unwrap();
        (dir, workspace)
    }

    #[test]
    fn busca_lexical_acha_nota() {
        let (dir, workspace) = demo("lex");
        let hits = search(&workspace, &Config::default(), "retry", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "gotcha");
        assert!(hits[0].text.contains("retry"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn conta_por_kind() {
        let (dir, workspace) = demo("kind");
        let kinds = por_kind(&workspace).unwrap();
        assert!(kinds.iter().any(|(k, n)| k == "gotcha" && *n == 1));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn hash_muda_com_texto() {
        let (dir, workspace) = demo("hash");
        let notas = notas(&workspace).unwrap();
        let h1 = hash_nota(&notas[0].0, &notas[0].1);
        let mut outra = notas[0].1.clone();
        outra.text.push_str(" mudou");
        let h2 = hash_nota(&notas[0].0, &outra);
        assert_ne!(h1, h2);
        std::fs::remove_dir_all(dir).ok();
    }
}
