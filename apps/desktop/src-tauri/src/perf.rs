//! Diagnostico de inicializacao do desktop.
//!
//! Grava JSONL em `%APPDATA%/dev.c2.desktop/perf.log` para medir o cold start
//! sem depender do console. Eventos de `workspace_status` so sao gravados nos
//! primeiros 30 s para nao poluir o arquivo com o poll de 2,5 s.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

static INICIO: OnceLock<Instant> = OnceLock::new();

/// Marca o inicio do processo (chamado no topo de `run`).
pub fn marcar_inicio() {
    INICIO.get_or_init(Instant::now);
}

pub fn elapsed_ms() -> u64 {
    INICIO
        .get()
        .map(|inicio| inicio.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

fn caminho_log(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("perf.log"))
}

pub fn registrar(app: &AppHandle, evento: &str, dados: Value) {
    let elapsed = elapsed_ms();
    if evento == "workspace_status" && elapsed > 30_000 {
        return;
    }
    let Some(caminho) = caminho_log(app) else {
        return;
    };
    if let Some(pai) = caminho.parent() {
        let _ = std::fs::create_dir_all(pai);
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duracao| duracao.as_millis() as u64)
        .unwrap_or(0);
    let linha = json!({
        "ts_ms": ts,
        "elapsed_ms": elapsed,
        "event": evento,
        "data": dados,
    });
    if let Ok(mut arquivo) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&caminho)
    {
        let _ = writeln!(arquivo, "{linha}");
    }
}
