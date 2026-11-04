//! Migracao de metadados legados para o schema v1 (bead `c2-e11t2`).
//!
//! O prototipo bash gravava `{"file","hash","mtime","semantic":{why,when,goal,summary}}`
//! (sem `schema_version`, sem `what`/`responsibility`, sem `links[]`). O leitor
//! ja tolera isso; aqui reescrevemos no formato canonico, guardando uma copia
//! do original em `.c2/backups/<epoch>/` para reversao.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::cleaner;
use crate::discover;
use crate::error::{IoContext, Result};
use crate::schema::{source_path_for, MetaFile};
use crate::state::now_secs;
use crate::workspace::Workspace;

/// Resultado da migracao.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MigrateReport {
    /// Metadados encontrados.
    pub scanned: usize,
    /// Ja estavam no schema v1.
    pub already_v1: usize,
    /// Convertidos (ou que seriam, em `dry_run`).
    pub migrated: usize,
    /// Falhas por arquivo.
    pub failed: Vec<String>,
    /// Pasta de backup (somente quando `apply`).
    pub backup_dir: Option<PathBuf>,
}

/// Converte os metadados legados para o schema v1.
///
/// Sem `apply` apenas conta (dry-run). Com `apply` faz backup do original e
/// reescreve atomicamente.
pub fn migrate(workspace: &Workspace, apply: bool) -> Result<MigrateReport> {
    let metadados = cleaner::metadata_files(workspace.root())?;
    let backup_dir = workspace
        .c2_dir()
        .join("backups")
        .join(now_secs().to_string());

    let mut report = MigrateReport {
        scanned: metadados.len(),
        ..Default::default()
    };

    for caminho in metadados {
        let bytes = match fs::read(&caminho) {
            Ok(bytes) => bytes,
            Err(err) => {
                report.failed.push(format!("{}: {err}", caminho.display()));
                continue;
            }
        };

        let value: Value = match serde_json::from_slice(crate::schema::strip_bom(&bytes)) {
            Ok(value) => value,
            Err(err) => {
                report
                    .failed
                    .push(format!("{}: JSON invalido ({err})", caminho.display()));
                continue;
            }
        };

        if value.get("schema_version").and_then(Value::as_u64) == Some(1) {
            report.already_v1 += 1;
            continue;
        }

        let mut meta = MetaFile::from_value(&value);
        if meta.file.is_empty() {
            let origem = source_path_for(&caminho);
            meta.file = discover::relative_unix(workspace.root(), &origem);
        }

        if meta.semantic.is_empty() {
            report.failed.push(format!(
                "{}: sem dados semanticos para migrar",
                caminho.display()
            ));
            continue;
        }
        if meta.hash.is_empty() {
            report
                .failed
                .push(format!("{}: sem hash, rode c2 reindex", caminho.display()));
            continue;
        }

        if apply {
            let relativo = meta.file.replace('/', std::path::MAIN_SEPARATOR_STR);
            let backup = backup_dir.join(format!("{relativo}.c2.json"));
            if let Some(pai) = backup.parent() {
                fs::create_dir_all(pai).at(pai)?;
            }
            fs::write(&backup, &bytes).at(&backup)?;

            match meta.write_atomic(&caminho) {
                Ok(()) => {}
                Err(err) => {
                    report.failed.push(format!("{}: {err}", caminho.display()));
                    continue;
                }
            }
        }

        report.migrated += 1;
    }

    if apply {
        report.backup_dir = Some(backup_dir);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("c2-migrate-{tag}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Metadado no formato do prototipo bash.
    fn legado() -> &'static str {
        r#"{
            "file": "src/a.rs",
            "hash": "sha256:abc",
            "mtime": 1700000000,
            "semantic": {"why": "porque sim", "when": "quando", "goal": "objetivo", "summary": "resumo"}
        }"#
    }

    fn preparar(tag: &str) -> (PathBuf, Workspace) {
        let base = tmp(tag);
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/a.rs"), "fn a() {}").unwrap();
        fs::write(base.join("src/a.rs.c2.json"), legado()).unwrap();
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        (base, workspace)
    }

    #[test]
    fn dry_run_conta_sem_gravar() {
        let (base, workspace) = preparar("dry");
        let report = migrate(&workspace, false).unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.migrated, 1);
        assert!(report.backup_dir.is_none());

        let bruto = fs::read_to_string(base.join("src/a.rs.c2.json")).unwrap();
        assert!(!bruto.contains("schema_version"), "dry-run nao deve gravar");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn aplica_backup_e_reescreve_no_schema_v1() {
        let (base, workspace) = preparar("apply");
        let report = migrate(&workspace, true).unwrap();
        assert_eq!(report.migrated, 1);
        assert!(report.failed.is_empty());

        let meta = MetaFile::read(&base.join("src/a.rs.c2.json")).unwrap();
        assert_eq!(meta.schema_version, 1);
        assert_eq!(meta.semantic.why, "porque sim");
        assert_eq!(meta.semantic.goal, "objetivo");
        assert_eq!(meta.file, "src/a.rs");

        let backup = report.backup_dir.unwrap().join("src").join("a.rs.c2.json");
        assert!(backup.is_file(), "backup do original deve existir");
        let original = fs::read_to_string(&backup).unwrap();
        assert!(original.contains("\"when\""));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn segunda_execucao_nao_migra_de_novo() {
        let (base, workspace) = preparar("idem");
        migrate(&workspace, true).unwrap();
        let report = migrate(&workspace, false).unwrap();
        assert_eq!(report.already_v1, 1);
        assert_eq!(report.migrated, 0);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn metadado_sem_semantica_falha_sem_gravar() {
        let base = tmp("vazio");
        fs::write(base.join("a.rs"), "x").unwrap();
        fs::write(base.join("a.rs.c2.json"), r#"{"file":"a.rs","hash":"h"}"#).unwrap();
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();

        let report = migrate(&workspace, true).unwrap();
        assert_eq!(report.migrated, 0);
        assert_eq!(report.failed.len(), 1);
        assert!(!fs::read_to_string(base.join("a.rs.c2.json"))
            .unwrap()
            .contains("schema_version"));
        fs::remove_dir_all(&base).ok();
    }
}
