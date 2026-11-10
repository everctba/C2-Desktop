//! Runtime separado de embeddings (`llama-server --embeddings`).
//!
//! Task `c2-kga.3`: processo isolado do Gemma, com CPU/GPU, health, status,
//! start/stop e endpoint `/v1/embeddings`. O modelo e o GGUF bge-m3 escolhido
//! na `c2-kga.1`; o cache fica em `embedding.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;

use crate::config::{Config, EmbeddingConfig};
use crate::error::{C2Error, IoContext, Result};
use crate::runtime;
use crate::workspace::Workspace;

/// Estado do runtime de embeddings exposto para CLI/UI.
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingStatus {
    pub enabled: bool,
    pub healthy: bool,
    pub pid: Option<u32>,
    pub model_path: Option<String>,
    pub model_id: String,
    pub dim: usize,
    pub device: String,
    /// Metadados com vetor válido no cache.
    pub vectors: usize,
    /// Total de metadados do workspace.
    pub files: usize,
    pub port: u16,
    pub binary: Option<String>,
    pub last_error: Option<String>,
}

pub fn status(workspace: &Workspace, config: &Config) -> EmbeddingStatus {
    let embedding = &config.index.embeddings;
    let pid = read_pid(workspace);
    let (files, vectors) = crate::embedding_index::stats(workspace, config).unwrap_or((0, 0));
    EmbeddingStatus {
        enabled: embedding.enabled,
        healthy: health(embedding),
        pid,
        model_path: embedding.model_path.clone(),
        model_id: embedding.model_id.clone(),
        dim: embedding.dim,
        device: embedding.device.clone(),
        vectors,
        files,
        port: embedding.port,
        binary: find_server(workspace).map(|path| path.display().to_string()),
        last_error: last_error(workspace),
    }
}

/// Sobe o runtime se ainda não estiver saudável. Retorna o estado final.
pub fn ensure(workspace: &Workspace, config: &Config) -> Result<EmbeddingStatus> {
    let embedding = &config.index.embeddings;
    if !embedding.enabled {
        return Err(C2Error::InvalidConfig(
            "busca semantica desativada (index.embeddings.enabled=false)".to_string(),
        ));
    }
    if health(embedding) {
        return Ok(status(workspace, config));
    }
    start(workspace, config)
}

pub fn start(workspace: &Workspace, config: &Config) -> Result<EmbeddingStatus> {
    let embedding = &config.index.embeddings;
    let model = embedding
        .model_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| {
            C2Error::InvalidConfig(
                "modelo de embeddings nao configurado (index.embeddings.model_path)".to_string(),
            )
        })?;
    let model = PathBuf::from(model);
    if !model.is_file() {
        return Err(C2Error::InvalidConfig(format!(
            "modelo de embeddings nao encontrado: {}",
            model.display()
        )));
    }
    let binary = find_server(workspace).ok_or_else(|| {
        C2Error::InvalidConfig("llama-server nao encontrado para embeddings".to_string())
    })?;
    if runtime::porta_em_uso("127.0.0.1", embedding.port) {
        return Err(C2Error::InvalidConfig(format!(
            "porta {} ja esta em uso para embeddings",
            embedding.port
        )));
    }

    fs::create_dir_all(workspace.logs_dir()).at(&workspace.logs_dir())?;
    let out_path = workspace.logs_dir().join("embedding.out.log");
    let err_path = workspace.logs_dir().join("embedding.err.log");
    let out = fs::File::create(&out_path).at(&out_path)?;
    let err = fs::File::create(&err_path).at(&err_path)?;
    let ngl = if device_label(embedding) == "gpu" {
        embedding.gpu_layers
    } else {
        0
    };
    let mut command = Command::new(&binary);
    command
        .arg("--embeddings")
        .arg("--pooling")
        .arg(&embedding.pooling)
        .arg("--model")
        .arg(&model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(embedding.port.to_string())
        .arg("--no-webui")
        .arg("-c")
        .arg(embedding.ctx_size.to_string())
        .arg("-ngl")
        .arg(ngl.to_string())
        .arg("-t")
        .arg(embedding.threads.to_string())
        // Resumos semanticos podem passar de 512 tokens; o batch fisico
        // padrao do llama-server rejeitava esses inputs.
        .arg("-b")
        .arg("2048")
        .arg("-ub")
        .arg("2048")
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    let child = crate::process::spawn(command).map_err(|erro| {
        C2Error::InvalidConfig(format!("falha ao iniciar runtime de embeddings: {erro}"))
    })?;
    write_pid(workspace, child.id())?;

    let limite = Instant::now() + Duration::from_secs(120);
    while Instant::now() < limite {
        if health(embedding) {
            return Ok(status(workspace, config));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = stop(workspace);
    Err(C2Error::InvalidConfig(
        "runtime de embeddings nao ficou saudavel em 120s; veja .c2/logs/embedding.err.log"
            .to_string(),
    ))
}

pub fn stop(workspace: &Workspace) -> Result<bool> {
    let Some(pid) = read_pid(workspace) else {
        return Ok(false);
    };
    #[cfg(windows)]
    let status = {
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::process::status(command)
    };
    #[cfg(not(windows))]
    let status = {
        let mut command = Command::new("kill");
        command
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::process::status(command)
    };
    let _ = fs::remove_file(pid_path(workspace));
    match status {
        Ok(exit) => Ok(exit.success()),
        Err(_) => Ok(false),
    }
}

/// Gera o embedding de um texto pelo runtime local.
pub fn embed(config: &Config, texto: &str) -> Result<Vec<f32>> {
    let embedding = &config.index.embeddings;
    if !health(embedding) {
        return Err(C2Error::InvalidConfig(
            "runtime de embeddings nao esta saudavel".to_string(),
        ));
    }
    let url = format!("http://127.0.0.1:{}/v1/embeddings", embedding.port);
    let body = json!({ "input": texto, "model": embedding.model_id });
    let resposta = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .build()
        .post(&url)
        .send_json(body)
        .map_err(|erro| {
            C2Error::InvalidConfig(format!("falha no endpoint de embeddings: {erro}"))
        })?;
    let json: serde_json::Value = resposta.into_json().map_err(|erro| {
        C2Error::InvalidConfig(format!("resposta de embeddings invalida: {erro}"))
    })?;
    let vetor = json
        .pointer("/data/0/embedding")
        .and_then(|valor| valor.as_array())
        .ok_or_else(|| C2Error::InvalidConfig("resposta sem data[0].embedding".to_string()))?
        .iter()
        .map(|valor| valor.as_f64().map(|n| n as f32))
        .collect::<Option<Vec<f32>>>()
        .ok_or_else(|| C2Error::InvalidConfig("embedding com valor invalido".to_string()))?;
    if vetor.len() != embedding.dim {
        return Err(C2Error::InvalidConfig(format!(
            "dimensao inesperada: {} (esperado {})",
            vetor.len(),
            embedding.dim
        )));
    }
    Ok(crate::embedding::normalize(&vetor))
}

pub fn health(embedding: &EmbeddingConfig) -> bool {
    if !runtime::porta_em_uso("127.0.0.1", embedding.port) {
        return false;
    }
    let url = format!("http://127.0.0.1:{}/health", embedding.port);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    matches!(agent.get(&url).call(), Ok(resposta) if resposta.status() == 200)
}

/// Localiza `llama-server`: env, runtime ao lado do executável, workspace ou PATH.
pub fn find_server(workspace: &Workspace) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("C2_LLAMA_SERVER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let nome = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidato in [
                dir.join("runtime").join(nome),
                dir.join("..").join("runtime").join(nome),
            ] {
                if candidato.is_file() {
                    return Some(candidato);
                }
            }
        }
    }
    let local = workspace.c2_dir().join("runtime").join(nome);
    if local.is_file() {
        return Some(local);
    }
    runtime::which("llama-server")
}

fn pid_path(workspace: &Workspace) -> PathBuf {
    workspace.c2_dir().join("embedding.pid")
}

fn read_pid(workspace: &Workspace) -> Option<u32> {
    let texto = fs::read_to_string(pid_path(workspace)).ok()?;
    let pid = texto.trim().parse::<u32>().ok()?;
    if runtime::pid_alive(pid) {
        Some(pid)
    } else {
        None
    }
}

fn write_pid(workspace: &Workspace, pid: u32) -> Result<()> {
    let path = pid_path(workspace);
    fs::write(&path, pid.to_string()).at(&path)?;
    Ok(())
}

fn last_error(workspace: &Workspace) -> Option<String> {
    let texto = fs::read_to_string(workspace.logs_dir().join("embedding.err.log")).ok()?;
    texto
        .lines()
        .rev()
        .map(str::trim)
        .find(|linha| {
            !linha.is_empty()
                && (linha.contains(" E ") || linha.to_ascii_lowercase().contains("error"))
        })
        .or_else(|| {
            texto
                .lines()
                .rev()
                .map(str::trim)
                .find(|linha| !linha.is_empty())
        })
        .map(|linha| linha.chars().take(240).collect())
}

/// Caminho usado nos testes para checar os argumentos de CPU/GPU sem subir
/// um processo. Mantido público para diagnóstico.
pub fn device_label(embedding: &EmbeddingConfig) -> &'static str {
    if embedding.device.eq_ignore_ascii_case("gpu") {
        "gpu"
    } else {
        "cpu"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_label_normaliza_cpu() {
        let mut cfg = EmbeddingConfig::default();
        assert_eq!(device_label(&cfg), "cpu");
        cfg.device = "GPU".to_string();
        assert_eq!(device_label(&cfg), "gpu");
    }

    #[test]
    fn defaults_do_embedding_sao_seguros() {
        let cfg = EmbeddingConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.dim, 1024);
        assert_eq!(cfg.pooling, "cls");
        assert_eq!(cfg.port, 8199);
    }

    #[test]
    fn find_server_nao_explode_sem_runtime() {
        let dir = std::env::temp_dir().join(format!("c2-embed-server-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ws = Workspace::at(dir.clone());
        let _ = find_server(&ws);
        fs::remove_dir_all(dir).ok();
    }
}
