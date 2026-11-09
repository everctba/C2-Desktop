//! Inteligência semântica sobre os vetores: similares, domínios, duplicados,
//! links sugeridos e co-mudança (tasks `c2-kga.6`..`c2-kga.9`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::cleaner;
use crate::config::Config;
use crate::embedding::{self, EmbeddingCache};
use crate::error::{C2Error, Result};
use crate::schema::MetaFile;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub file: String,
    pub summary: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicatePair {
    pub a: String,
    pub b: String,
    pub score: f32,
    pub linked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenericFile {
    pub file: String,
    pub summary: String,
    pub what: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestion {
    pub from: String,
    pub to: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoChange {
    pub file: String,
    pub commits: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateReport {
    pub pairs: Vec<DuplicatePair>,
    pub generic: Vec<GenericFile>,
}

struct Vetor {
    meta: MetaFile,
    valores: Vec<f32>,
}

fn carregar(workspace: &Workspace, config: &Config) -> Result<Vec<Vetor>> {
    let cfg = &config.index.embeddings;
    let cache = EmbeddingCache::new(workspace, &cfg.model_id, cfg.dim);
    let mut saida = Vec::new();
    for meta_path in cleaner::metadata_files(workspace.root())? {
        let Ok(meta) = MetaFile::read(&meta_path) else {
            continue;
        };
        let Some(valores) = cache.load(&meta.file, &meta.hash) else {
            continue;
        };
        saida.push(Vetor { meta, valores });
    }
    Ok(saida)
}

/// Arquivos similares a `file`, ordenados por cosine.
pub fn similar(
    workspace: &Workspace,
    config: &Config,
    file: &str,
    limit: usize,
) -> Result<Vec<Neighbor>> {
    let vetores = carregar(workspace, config)?;
    let alvo = vetores
        .iter()
        .find(|item| item.meta.file == file)
        .ok_or_else(|| {
            C2Error::InvalidConfig(format!("arquivo sem vetor de embedding: {file}"))
        })?;
    let mut vizinhos = Vec::new();
    for item in &vetores {
        if item.meta.file == file {
            continue;
        }
        if let Some(score) = embedding::cosine(&alvo.valores, &item.valores) {
            if score >= config.index.embeddings.min_score {
                vizinhos.push(Neighbor {
                    file: item.meta.file.clone(),
                    summary: item.meta.semantic.summary.clone(),
                    score,
                });
            }
        }
    }
    vizinhos.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    vizinhos.truncate(limit);
    Ok(vizinhos)
}

/// Agrupa vizinhos pelo domínio estrutural do caminho.
pub fn domains(neighbors: &[Neighbor]) -> BTreeMap<String, Vec<String>> {
    let mut grupos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for neighbor in neighbors {
        let dominio = domain_of(&neighbor.file);
        grupos.entry(dominio).or_default().push(neighbor.file.clone());
    }
    grupos
}

pub fn domain_of(file: &str) -> String {
    let partes: Vec<&str> = file.split('/').collect();
    if partes.len() >= 2 {
        if partes[0] == "src" || partes[0] == "crates" || partes[0] == "apps" {
            return partes[1].to_string();
        }
        return partes[0].to_string();
    }
    "(raiz)".to_string()
}

/// Pares com cosine alto (prováveis duplicados) e metadados genéricos.
pub fn duplicates(
    workspace: &Workspace,
    config: &Config,
    threshold: f32,
) -> Result<DuplicateReport> {
    let vetores = carregar(workspace, config)?;
    let mut pairs = Vec::new();
    for (i, a) in vetores.iter().enumerate() {
        for b in vetores.iter().skip(i + 1) {
            if let Some(score) = embedding::cosine(&a.valores, &b.valores) {
                if score >= threshold {
                    let linked = a.meta.links.iter().any(|link| link.to == b.meta.file)
                        || b.meta.links.iter().any(|link| link.to == a.meta.file);
                    pairs.push(DuplicatePair {
                        a: a.meta.file.clone(),
                        b: b.meta.file.clone(),
                        score,
                        linked,
                    });
                }
            }
        }
    }
    pairs.sort_by(|x, y| y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal));
    pairs.truncate(100);

    let mut generic = Vec::new();
    for item in &vetores {
        let resumo = item.meta.semantic.summary.trim();
        let what = item.meta.semantic.what.trim();
        if resumo.chars().count() < 32 || what.chars().count() < 8 {
            generic.push(GenericFile {
                file: item.meta.file.clone(),
                summary: resumo.to_string(),
                what: what.to_string(),
            });
        }
    }
    Ok(DuplicateReport { pairs, generic })
}

/// Sugere links para pares fortes que ainda não estão ligados no grafo.
pub fn link_suggestions(
    workspace: &Workspace,
    config: &Config,
    threshold: f32,
    limit: usize,
) -> Result<Vec<LinkSuggestion>> {
    let report = duplicates(workspace, config, threshold)?;
    let sugestoes = report
        .pairs
        .into_iter()
        .filter(|pair| !pair.linked)
        .map(|pair| LinkSuggestion {
            from: pair.a,
            to: pair.b,
            score: pair.score,
        })
        .take(limit)
        .collect();
    Ok(sugestoes)
}

/// Arquivos que costumam mudar nos mesmos commits (`git log`).
pub fn cochange(workspace: &Workspace, file: &str, limit: usize) -> Result<Vec<CoChange>> {
    if !workspace.root().join(".git").is_dir() {
        return Err(C2Error::InvalidConfig(
            "workspace nao e um repositorio git".to_string(),
        ));
    }
    let saida = Command::new("git")
        .args([
            "-C",
            &workspace.root().to_string_lossy(),
            "log",
            "--no-merges",
            "--pretty=format:--C2--",
            "--name-only",
            "--since=1.year",
        ])
        .output()
        .map_err(|erro| C2Error::InvalidConfig(format!("git log falhou: {erro}")))?;
    if !saida.status.success() {
        return Err(C2Error::InvalidConfig(
            "git log falhou (workspace sem historico?)".to_string(),
        ));
    }
    let texto = String::from_utf8_lossy(&saida.stdout);
    let mut contagem: HashMap<String, usize> = HashMap::new();
    let mut commit: HashSet<String> = HashSet::new();
    let mut commits_com_alvo = 0usize;
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha == "--C2--" {
            if commit.contains(file) {
                commits_com_alvo += 1;
                for outro in commit.iter() {
                    if outro != file {
                        *contagem.entry(outro.clone()).or_default() += 1;
                    }
                }
            }
            commit.clear();
        } else if !linha.is_empty() {
            if !linha.starts_with("--") && Path::new(linha).extension().is_some() {
                commit.insert(linha.replace('\\', "/"));
            }
        }
    }
    if commit.contains(file) {
        commits_com_alvo += 1;
        for outro in commit.iter() {
            if outro != file {
                *contagem.entry(outro.clone()).or_default() += 1;
            }
        }
    }
    let mut saida: Vec<CoChange> = contagem
        .into_iter()
        .map(|(file, commits)| CoChange { file, commits })
        .collect();
    saida.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.file.cmp(&b.file)));
    saida.truncate(limit);
    let _ = commits_com_alvo;
    Ok(saida)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Link, Semantic};
    use std::fs;

    fn setup(tag: &str) -> (Workspace, Config) {
        let dir = std::env::temp_dir().join(format!(
            "c2-insights-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join(".c2")).unwrap();
        let mut config = Config::default();
        config.index.embeddings.enabled = true;
        config.index.embeddings.dim = 4;
        config.index.embeddings.min_score = 0.5;
        config.write_atomic(&dir.join(".c2/config.json")).unwrap();
        (Workspace::at(dir), config)
    }

    fn adicionar(ws: &Workspace, config: &Config, rel: &str, vetor: [f32; 4], links: Vec<&str>) {
        let meta = MetaFile::new(
            rel,
            "sha256:x",
            1,
            0,
            Semantic {
                what: "coisa".to_string(),
                why: "porque".to_string(),
                goal: "objetivo".to_string(),
                summary: "resumo curto".to_string(),
                responsibility: "dono".to_string(),
            },
            links
                .into_iter()
                .map(|to| Link {
                    to: to.to_string(),
                    kind: "imports".to_string(),
                })
                .collect(),
        );
        let path = crate::schema::meta_path_for(&ws.root().join(rel));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        meta.write_atomic(&path).unwrap();
        let cache = EmbeddingCache::new(ws, &config.index.embeddings.model_id, config.index.embeddings.dim);
        cache.store(rel, "sha256:x", &vetor).unwrap();
    }

    #[test]
    fn similares_e_dominios() {
        let (ws, config) = setup("sim");
        adicionar(&ws, &config, "src/pagamento/a.rs", [1.0, 0.0, 0.0, 0.0], vec![]);
        adicionar(&ws, &config, "src/pagamento/b.rs", [0.9, 0.1, 0.0, 0.0], vec![]);
        adicionar(&ws, &config, "src/auth/c.rs", [0.0, 0.0, 1.0, 0.0], vec![]);
        let vizinhos = similar(&ws, &config, "src/pagamento/a.rs", 5).unwrap();
        assert_eq!(vizinhos[0].file, "src/pagamento/b.rs");
        assert_eq!(domain_of("src/pagamento/a.rs"), "pagamento");
        let grupos = domains(&vizinhos);
        assert!(grupos.contains_key("pagamento"));
        fs::remove_dir_all(ws.root()).ok();
    }

    #[test]
    fn duplicados_e_links() {
        let (ws, config) = setup("dup");
        adicionar(&ws, &config, "src/a.rs", [1.0, 0.0, 0.0, 0.0], vec![]);
        adicionar(&ws, &config, "src/b.rs", [1.0, 0.0, 0.0, 0.0], vec![]);
        let report = duplicates(&ws, &config, 0.95).unwrap();
        assert_eq!(report.pairs.len(), 1);
        assert!(!report.pairs[0].linked);
        assert!(!report.generic.is_empty());
        let links = link_suggestions(&ws, &config, 0.95, 5).unwrap();
        assert_eq!(links.len(), 1);
        fs::remove_dir_all(ws.root()).ok();
    }
}
