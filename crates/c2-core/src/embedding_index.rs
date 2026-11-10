//! Indexa os resumos `.c2.json` em vetores (task `c2-kga.4`).
//!
//! Um vetor por arquivo, incremental por hash do metadado. O provedor de
//! embedding é injetado para permitir testes sem subir llama-server.

use std::collections::HashSet;

use serde::Serialize;

use crate::cleaner;
use crate::config::Config;
use crate::embedding::{self, EmbeddingCache};
use crate::embedding_runtime;
use crate::error::{C2Error, Result};
use crate::schema::{meta_path_for, MetaFile};
use crate::workspace::Workspace;

/// Resultado de uma passada de indexação de embeddings.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EmbeddingIndexReport {
    pub files: usize,
    /// Vetores gerados nesta passada.
    pub embedded: usize,
    /// Vetores reaproveitados pelo hash.
    pub cached: usize,
    /// Arquivos sem campos semânticos ou sem hash.
    pub skipped: usize,
    pub errors: usize,
    pub last_error: Option<String>,
    pub model_id: String,
    pub dim: usize,
}

pub fn run(
    workspace: &Workspace,
    config: &Config,
    full: bool,
    progress: impl FnMut(&str, usize, usize),
) -> Result<EmbeddingIndexReport> {
    let embedding_cfg = &config.index.embeddings;
    if !embedding_cfg.enabled {
        return Err(C2Error::InvalidConfig(
            "busca semantica desativada (index.embeddings.enabled=false)".to_string(),
        ));
    }
    let status = embedding_runtime::ensure(workspace, config)?;
    if !status.healthy {
        return Err(C2Error::InvalidConfig(
            "runtime de embeddings nao esta saudavel".to_string(),
        ));
    }
    run_with(workspace, config, full, progress, |texto| {
        embedding_runtime::embed(config, texto)
    })
}

/// Variante testável: o gerador de vetores é uma closure.
pub fn run_with(
    workspace: &Workspace,
    config: &Config,
    full: bool,
    mut progress: impl FnMut(&str, usize, usize),
    mut embedder: impl FnMut(&str) -> Result<Vec<f32>>,
) -> Result<EmbeddingIndexReport> {
    let cfg = &config.index.embeddings;
    let model_id = cfg.model_id.clone();
    let dim = cfg.dim;
    if dim == 0 {
        return Err(C2Error::InvalidConfig(
            "index.embeddings.dim deve ser > 0".to_string(),
        ));
    }
    let cache = EmbeddingCache::new(workspace, &model_id, dim);
    let metas = cleaner::metadata_files(workspace.root())?;
    let mut report = EmbeddingIndexReport {
        files: metas.len(),
        model_id,
        dim,
        ..EmbeddingIndexReport::default()
    };
    let mut existentes = HashSet::new();

    for (posicao, meta_path) in metas.iter().enumerate() {
        let meta = match MetaFile::read(meta_path) {
            Ok(meta) => meta,
            Err(err) => {
                report.errors += 1;
                report.last_error = Some(format!("{}: {err}", meta_path.display()));
                progress("", posicao + 1, metas.len());
                continue;
            }
        };
        existentes.insert(meta.file.clone());
        if meta.hash.trim().is_empty() || meta.semantic.is_empty() {
            report.skipped += 1;
            progress(&meta.file, posicao + 1, metas.len());
            continue;
        }
        if !full && cache.load(&meta.file, &meta.hash).is_some() {
            report.cached += 1;
            progress(&meta.file, posicao + 1, metas.len());
            continue;
        }
        let texto = embedding::text_for(&meta);
        if texto.trim().is_empty() {
            report.skipped += 1;
            progress(&meta.file, posicao + 1, metas.len());
            continue;
        }
        let mut tentativa = 0;
        let mut resultado = embedder(&texto);
        while resultado.is_err() && tentativa < 2 {
            tentativa += 1;
            std::thread::sleep(std::time::Duration::from_millis(250 * tentativa));
            resultado = embedder(&texto);
        }
        match resultado {
            Ok(vetor) => {
                if vetor.len() != dim {
                    report.errors += 1;
                    report.last_error = Some(format!(
                        "{}: dimensao {} (esperado {dim})",
                        meta.file,
                        vetor.len()
                    ));
                } else if let Err(err) = cache.store(&meta.file, &meta.hash, &vetor) {
                    report.errors += 1;
                    report.last_error = Some(format!("{}: {err}", meta.file));
                } else {
                    report.embedded += 1;
                }
            }
            Err(err) => {
                report.errors += 1;
                report.last_error = Some(format!("{}: {err}", meta.file));
            }
        }
        progress(&meta.file, posicao + 1, metas.len());
    }

    // Remove vetores de arquivos que não existem mais.
    let existentes: Vec<String> = existentes.into_iter().collect();
    if let Err(err) = cache.prune(&existentes) {
        report.errors += 1;
        report.last_error = Some(format!("prune: {err}"));
    }
    Ok(report)
}

/// Quantos arquivos têm metadado e quantos têm vetor válido no cache.
pub fn stats(workspace: &Workspace, config: &Config) -> Result<(usize, usize)> {
    let cfg = &config.index.embeddings;
    let cache = EmbeddingCache::new(workspace, &cfg.model_id, cfg.dim);
    let mut total = 0usize;
    let mut com_vetor = 0usize;
    for meta_path in cleaner::metadata_files(workspace.root())? {
        let Ok(meta) = MetaFile::read(&meta_path) else {
            continue;
        };
        total += 1;
        if cache.load(&meta.file, &meta.hash).is_some() {
            com_vetor += 1;
        }
    }
    Ok((total, com_vetor))
}

/// Caminho do metadado ao lado de um arquivo relativo (utilitário da CLI/UI).
pub fn meta_for(workspace: &Workspace, rel: &str) -> std::path::PathBuf {
    meta_path_for(&workspace.root().join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Semantic;
    use std::fs;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let carimbo = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("c2-embed-index-{tag}-{carimbo}"));
        fs::create_dir_all(dir.join(".c2")).unwrap();
        dir
    }

    fn workspace(config: &Config) -> Workspace {
        let dir = tmp("ws");
        let ws = Workspace::at(dir);
        config.write_atomic(&ws.config_path()).unwrap();
        ws
    }

    fn gravar_meta(ws: &Workspace, rel: &str, hash: &str) {
        let meta = MetaFile::new(
            rel,
            hash,
            1,
            0,
            Semantic {
                what: "what".to_string(),
                why: "why".to_string(),
                goal: "goal".to_string(),
                summary: "summary".to_string(),
                responsibility: "responsibility".to_string(),
            },
            Vec::new(),
        );
        let path = meta_path_for(&ws.root().join(rel));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        meta.write_atomic(&path).unwrap();
    }

    #[test]
    fn indexa_e_reaproveita_por_hash() {
        let mut config = Config::default();
        config.index.embeddings.enabled = true;
        config.index.embeddings.dim = 4;
        let ws = workspace(&config);
        gravar_meta(&ws, "src/a.rs", "sha256:a");
        let mut chamadas = 0;
        let report = run_with(
            &ws,
            &config,
            false,
            |_, _, _| {},
            |_| {
                chamadas += 1;
                Ok(vec![1.0, 0.0, 0.0, 0.0])
            },
        )
        .unwrap();
        assert_eq!(report.embedded, 1);
        assert_eq!(chamadas, 1);
        let report = run_with(
            &ws,
            &config,
            false,
            |_, _, _| {},
            |_| {
                chamadas += 1;
                Ok(vec![0.0, 1.0, 0.0, 0.0])
            },
        )
        .unwrap();
        assert_eq!(report.cached, 1);
        assert_eq!(report.embedded, 0);
        assert_eq!(chamadas, 1);
        fs::remove_dir_all(ws.root()).ok();
    }

    #[test]
    fn hash_novo_forca_novo_vetor() {
        let mut config = Config::default();
        config.index.embeddings.enabled = true;
        config.index.embeddings.dim = 4;
        let ws = workspace(&config);
        gravar_meta(&ws, "src/b.rs", "sha256:a");
        let mut chamadas = 0;
        run_with(
            &ws,
            &config,
            false,
            |_, _, _| {},
            |_| {
                chamadas += 1;
                Ok(vec![1.0, 0.0, 0.0, 0.0])
            },
        )
        .unwrap();
        gravar_meta(&ws, "src/b.rs", "sha256:b");
        let report = run_with(
            &ws,
            &config,
            false,
            |_, _, _| {},
            |_| {
                chamadas += 1;
                Ok(vec![1.0, 0.0, 0.0, 0.0])
            },
        )
        .unwrap();
        assert_eq!(report.embedded, 1);
        assert_eq!(chamadas, 2);
        fs::remove_dir_all(ws.root()).ok();
    }

    #[test]
    fn runtime_desligado_falha_com_mensagem() {
        let config = Config::default();
        let ws = workspace(&config);
        let err = run(&ws, &config, false, |_, _, _| {}).unwrap_err();
        assert!(err.to_string().contains("desativada"));
        fs::remove_dir_all(ws.root()).ok();
    }
}
