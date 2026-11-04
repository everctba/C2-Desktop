//! Remocao de metadados: equivalente dirigido ao `c2Cleaner.sh`.
//!
//! Diferencas propositais em relacao ao script do prototipo:
//! - nunca remove nada sem `apply` (o CLI chama com `--yes`);
//! - nao entra em `.git`, `.beads`, `target`, `node_modules` etc.;
//! - devolve relatorio do que seria removido e do que falhou.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::default_excluded_dirs;
use crate::error::{IoContext, Result};
use crate::schema::is_meta_path;

/// Relatorio da limpeza.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanupReport {
    /// Metadados encontrados, ordenados por caminho.
    pub found: Vec<PathBuf>,
    /// Metadados efetivamente removidos (vazio em modo `dry_run`).
    pub removed: Vec<PathBuf>,
    /// Falhas de remocao.
    pub failed: Vec<String>,
    /// `true` quando nada foi apagado.
    pub dry_run: bool,
}

/// Lista todos os `.c2.json` sob `root`, sem entrar nos diretorios excluidos.
pub fn metadata_files(root: &Path) -> Result<Vec<PathBuf>> {
    let excluded = default_excluded_dirs();
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if dir == root => return Err(crate::error::C2Error::io(&dir, err)),
            Err(_) => continue,
        };

        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if excluded.iter().any(|dir| dir == &name) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file() && is_meta_path(&path) {
                found.push(path);
            }
        }
    }

    found.sort();
    Ok(found)
}

/// Remove (ou apenas lista, com `dry_run`) os metadados do workspace.
pub fn clean(root: &Path, dry_run: bool) -> Result<CleanupReport> {
    let found = metadata_files(root)?;
    let mut report = CleanupReport {
        found: found.clone(),
        dry_run,
        ..Default::default()
    };

    if dry_run {
        return Ok(report);
    }

    for path in found {
        match fs::remove_file(&path).at(&path) {
            Ok(()) => report.removed.push(path),
            Err(_) => report.failed.push(path.display().to_string()),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("c2-clean-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"{}").unwrap();
    }

    #[test]
    fn lista_somente_metadados_fora_dos_excluidos() {
        let base = tmp("list");
        touch(&base.join("src").join("a.rs.c2.json"));
        touch(&base.join("b.py.c2.json"));
        touch(&base.join("node_modules").join("x.c2.json"));
        touch(&base.join(".git").join("y.c2.json"));
        touch(&base.join("a.rs"));

        let found = metadata_files(&base).unwrap();
        assert_eq!(found.len(), 2);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn dry_run_nao_apaga() {
        let base = tmp("dry");
        touch(&base.join("a.rs.c2.json"));
        let report = clean(&base, true).unwrap();
        assert_eq!(report.found.len(), 1);
        assert!(report.removed.is_empty());
        assert!(base.join("a.rs.c2.json").exists());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn apply_apaga_e_reporta() {
        let base = tmp("apply");
        touch(&base.join("a.rs.c2.json"));
        touch(&base.join("sub").join("b.rs.c2.json"));
        let report = clean(&base, false).unwrap();
        assert_eq!(report.removed.len(), 2);
        assert!(!base.join("a.rs.c2.json").exists());
        assert!(report.failed.is_empty());
        fs::remove_dir_all(&base).ok();
    }
}
