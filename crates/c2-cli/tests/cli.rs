//! Testes de integracao da CLI: exercitam o binario `c2` de verdade.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("c2-cli-{tag}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn c2() -> Command {
    Command::new(env!("CARGO_BIN_EXE_c2"))
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn ciclo_init_status_cleaner() {
    let base = tmp("ciclo");
    fs::write(base.join("a.rs"), "fn main() {}").unwrap();
    fs::write(base.join("README.md"), "# doc").unwrap();

    let out = c2().arg("--root").arg(&base).arg("init").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(base.join(".c2").join("config.json").is_file());
    assert!(base.join(".c2").join("logs").is_dir());

    // idempotente: a segunda execucao nao sobrescreve
    let out = c2().arg("--root").arg(&base).arg("init").output().unwrap();
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("ja existia"));

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["files"], 2);
    assert_eq!(value["metadata"], 0);
    assert_eq!(value["engine"]["running"], false);

    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("cleaner")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("0 metadados"));

    fs::remove_dir_all(&base).ok();
}

#[test]
fn cleaner_so_apaga_com_yes() {
    let base = tmp("cleaner");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();
    fs::write(base.join("a.rs"), "x").unwrap();
    let meta = base.join("a.rs.c2.json");
    fs::write(&meta, "{}").unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("cleaner")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(meta.exists(), "dry-run nao deve apagar");

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["cleaner", "--yes"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!meta.exists(), "--yes deve apagar");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn config_roundtrip_pela_cli() {
    let base = tmp("config");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "set", "model.port", "9099"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "get", "model.port"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(stdout_of(&out).trim(), "9099");

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["config", "get", "nao.existe"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "chave invalida deve falhar");

    fs::remove_dir_all(&base).ok();
}

#[test]
fn comando_nao_implementado_sai_com_3() {
    let base = tmp("stub");
    c2().arg("--root").arg(&base).arg("init").output().unwrap();

    let out = c2()
        .arg("--root")
        .arg(&base)
        .args(["grep", "qualquer coisa"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("c2-e8"));

    fs::remove_dir_all(&base).ok();
}

#[test]
fn status_sem_workspace_falha_com_mensagem() {
    let base = tmp("sem-ws");
    let out = c2()
        .arg("--root")
        .arg(&base)
        .arg("status")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nao inicializado"));
    fs::remove_dir_all(&base).ok();
}
