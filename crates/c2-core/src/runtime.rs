//! Ciclo de vida do `llama-server` (beads `c2-e4t2`, `c2-e4t3`).
//!
//! O C2 localiza o binario, valida, sobe, verifica saude, reinicia com backoff
//! e encerra o processo. Ordem de busca do binario:
//!
//! 1. `model.binary` na configuracao;
//! 2. `<workspace>/.c2/runtime/llama-server`;
//! 3. **runtime embarcado**: `<dir do C2>/runtime/llama-server` (e `../runtime`,
//!    para a CLI que fica em `cli/`);
//! 4. `llama-server` no PATH;
//! 5. backends do LM Studio (opt-in de desenvolvimento, `model.allow_lmstudio`).
//!
//! O item 4 existe so para desenvolvimento: o instalador do C2 vai embarcar o
//! runtime proprio (`c2-e12.1`), e entao o LM Studio deixa de ser necessario.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::{C2Error, IoContext, Result};
use crate::workspace::Workspace;

/// Binario localizado e o porque da escolha.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub binary: PathBuf,
    pub dll_dirs: Vec<PathBuf>,
    pub source: String,
}

/// Situacao do runtime.
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub healthy: bool,
    pub pid: Option<u32>,
    pub binary: Option<PathBuf>,
    pub source: Option<String>,
    pub version: Option<String>,
}

/// Resultado de `ensure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ensure {
    /// Ja estava saudavel, nada foi feito.
    Reused,
    /// Precisou subir (com o numero de tentativas).
    Started { attempts: u32 },
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Caminho do arquivo de PID do runtime.
pub fn pid_file(workspace: &Workspace) -> PathBuf {
    workspace.c2_dir().join("runtime.pid")
}

/// Verdadeiro quando o processo existe (Windows usa tasklist).
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// PID do runtime registrado e ainda vivo.
pub fn current_pid(workspace: &Workspace) -> Option<u32> {
    let text = fs::read_to_string(pid_file(workspace)).ok()?;
    let pid = text.trim().parse::<u32>().ok()?;
    if pid_alive(pid) {
        Some(pid)
    } else {
        None
    }
}

/// Verdadeiro quando o servidor responde em `/health`.
pub fn health(config: &Config) -> bool {
    let url = config.model.health_url();
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    matches!(agent.get(&url).call(), Ok(response) if response.status() == 200)
}

/// Localiza o binario seguindo a ordem documentada.
pub fn resolve_binary(workspace: &Workspace, config: &Config) -> Result<Resolved> {
    let dll_dirs = configured_dll_dirs(config)?;

    if let Some(path) = config
        .model
        .binary
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let binary = PathBuf::from(path);
        if binary.is_file() {
            return Ok(Resolved {
                binary,
                dll_dirs,
                source: "model.binary".to_string(),
            });
        }
        return Err(C2Error::InvalidConfig(format!(
            "model.binary aponta para um arquivo inexistente: {path}"
        )));
    }

    let local = workspace.c2_dir().join("runtime").join(exe_name("llama-server"));
    if local.is_file() {
        return Ok(Resolved {
            binary: local,
            dll_dirs,
            source: "workspace .c2/runtime".to_string(),
        });
    }

    if let Some(bundled) = bundled_runtime() {
        return Ok(Resolved {
            binary: bundled,
            dll_dirs,
            source: "runtime embarcado".to_string(),
        });
    }

    if let Some(found) = which("llama-server") {
        return Ok(Resolved {
            binary: found,
            dll_dirs,
            source: "PATH".to_string(),
        });
    }

    if config.model.allow_lmstudio {
        if let Some((binary, vendor)) = lmstudio_backend() {
            let mut dirs = dll_dirs;
            if let Some(vendor) = vendor {
                dirs.push(vendor);
            }
            return Ok(Resolved {
                binary,
                dll_dirs: dirs,
                source:
                    "LM Studio (fallback de desenvolvimento; c2-e12.1 embarca o runtime proprio)"
                        .to_string(),
            });
        }
    }

    Err(C2Error::InvalidConfig(format!(
        "llama-server nao encontrado. Defina `model.binary`, coloque o binario em \
         `{}` ou instale no PATH.",
        workspace.c2_dir().join("runtime").display()
    )))
}

/// `llama-server` embarcado ao lado do executavel do C2.
///
/// Procura `<exe_dir>/runtime` e `<exe_dir>/../runtime` — a CLI fica em
/// `cli/c2.exe` no pacote portatil, entao sobe um nivel.
fn bundled_runtime() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("runtime").join(exe_name("llama-server"));
    if candidate.is_file() {
        return Some(candidate);
    }
    let parent = dir.join("..").join("runtime").join(exe_name("llama-server"));
    if parent.is_file() {
        return Some(parent);
    }
    None
}

fn configured_dll_dirs(config: &Config) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in &config.model.dll_dirs {
        let path = PathBuf::from(entry);
        if !path.is_dir() {
            return Err(C2Error::InvalidConfig(format!(
                "model.dll_dirs aponta para um diretorio inexistente: {entry}"
            )));
        }
        dirs.push(path);
    }
    Ok(dirs)
}

/// Busca `name[.exe]` no PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let candidate = exe_name(name);
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(&candidate))
        .find(|candidate| candidate.is_file())
}

/// Backend do LM Studio mais recente com CUDA 12, se existir.
fn lmstudio_backend() -> Option<(PathBuf, Option<PathBuf>)> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let backends = PathBuf::from(home)
        .join(".lmstudio")
        .join("extensions")
        .join("backends");
    if !backends.is_dir() {
        return None;
    }

    let mut candidates: Vec<PathBuf> = fs::read_dir(&backends)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().contains("llama.cpp-"))
                    .unwrap_or(false)
                && path.join(exe_name("llama-server")).is_file()
        })
        .collect();
    candidates.sort();

    let binary = candidates.pop()?;
    let vendor = [
        "vendor/win-llama-cuda12-vendor-v2",
        "vendor/win-llama-cuda-vendor-v2",
        "vendor/win-llama-vulkan-vendor-v2",
    ]
    .iter()
    .map(|relative| backends.join(relative))
    .find(|path| path.is_dir());

    Some((binary.join(exe_name("llama-server")), vendor))
}

/// Monta o PATH do processo: DLLs configuradas primeiro.
fn path_with_dlls(dll_dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut all: Vec<PathBuf> = dll_dirs.to_vec();
    all.extend(std::env::split_paths(&current));
    std::env::join_paths(all).ok()
}

/// Executa `--version` e devolve a primeira linha util, validando o binario.
pub fn validate_binary(resolved: &Resolved) -> Result<String> {
    let mut command = Command::new(&resolved.binary);
    command.arg("--version");
    if let Some(path) = path_with_dlls(&resolved.dll_dirs) {
        command.env("PATH", path);
    }
    let output = command.output().map_err(|err| {
        C2Error::InvalidConfig(format!(
            "nao foi possivel executar {}: {err}",
            resolved.binary.display()
        ))
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };

    if !output.status.success() && text.trim().is_empty() {
        return Err(C2Error::InvalidConfig(format!(
            "{} saiu com {} e nao informou versao (DLLs faltando?)",
            resolved.binary.display(),
            output.status
        )));
    }

    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
        .ok_or_else(|| {
            C2Error::InvalidConfig(format!(
                "{} nao respondeu a --version",
                resolved.binary.display()
            ))
        })
}

/// Sobe o servidor, espera o health check e devolve o PID.
pub fn start(workspace: &Workspace, config: &Config, resolved: &Resolved) -> Result<u32> {
    let model = config.model.path.as_deref().ok_or_else(|| {
        C2Error::InvalidConfig("model.path nao configurado (caminho do .gguf)".to_string())
    })?;
    if !Path::new(model).is_file() {
        return Err(C2Error::InvalidConfig(format!(
            "modelo GGUF nao encontrado: {model}"
        )));
    }

    let logs = workspace.logs_dir();
    fs::create_dir_all(&logs).at(&logs)?;
    let out_path = logs.join("llama.out.log");
    let err_path = logs.join("llama.err.log");
    let out = fs::File::create(&out_path).at(&out_path)?;
    let err = fs::File::create(&err_path).at(&err_path)?;

    let mut command = Command::new(&resolved.binary);
    command
        .arg("-m")
        .arg(model)
        .arg("--host")
        .arg(&config.model.host)
        .arg("--port")
        .arg(config.model.port.to_string())
        .arg("-c")
        .arg(config.model.ctx_size.to_string())
        .arg("--no-webui")
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .stdin(Stdio::null());

    if config.model.gpu_layers > 0 {
        command.arg("-ngl").arg(config.model.gpu_layers.to_string());
    }
    if config.model.threads > 0 {
        command.arg("-t").arg(config.model.threads.to_string());
    }
    if let Some(path) = path_with_dlls(&resolved.dll_dirs) {
        command.env("PATH", path);
    }

    let child = command.spawn().map_err(|err| {
        C2Error::InvalidConfig(format!(
            "nao foi possivel iniciar {}: {err}",
            resolved.binary.display()
        ))
    })?;
    let pid = child.id();
    fs::write(pid_file(workspace), pid.to_string()).at(&pid_file(workspace))?;

    // Modelo de 3 GB em disco leva dezenas de segundos para carregar.
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if health(config) {
            return Ok(pid);
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = stop(workspace);
    let tail = fs::read_to_string(&err_path).unwrap_or_default();
    let tail: String = tail.lines().rev().take(8).collect::<Vec<_>>().join("\n");
    Err(C2Error::Llm(format!(
        "llama-server nao respondeu em 180s. Log ({err_path:?}):\n{tail}"
    )))
}

/// Garante o servidor no ar, reiniciando com backoff ate `model.max_retries`.
pub fn ensure(workspace: &Workspace, config: &Config) -> Result<Ensure> {
    if health(config) {
        return Ok(Ensure::Reused);
    }

    let resolved = resolve_binary(workspace, config)?;
    let attempts = config.model.max_retries.saturating_add(1);

    for attempt in 0..attempts {
        match start(workspace, config, &resolved) {
            Ok(_) => {
                return Ok(Ensure::Started {
                    attempts: attempt + 1,
                })
            }
            Err(err) if attempt + 1 == attempts => return Err(err),
            Err(_) => {
                std::thread::sleep(Duration::from_secs(1u64 << attempt.min(4)));
            }
        }
    }

    Err(C2Error::Llm(
        "nao foi possivel iniciar o llama-server".to_string(),
    ))
}

/// Encerra o runtime registrado. Devolve `true` se havia processo vivo.
pub fn stop(workspace: &Workspace) -> Result<bool> {
    let Some(pid) = current_pid(workspace) else {
        let _ = fs::remove_file(pid_file(workspace));
        return Ok(false);
    };

    #[cfg(windows)]
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(not(windows))]
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();

    let killeed = status.map(|status| status.success()).unwrap_or(false);

    // Espera o processo sumir antes de remover o PID.
    for _ in 0..20 {
        if !pid_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if pid_alive(pid) {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(windows))]
        {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        }
    }

    let _ = fs::remove_file(pid_file(workspace));
    Ok(killeed)
}

/// Resumo para `c2 status`.
pub fn status(workspace: &Workspace, config: &Config) -> RuntimeStatus {
    let healthy = health(config);
    let pid = current_pid(workspace);
    let resolved = resolve_binary(workspace, config).ok();
    let version = resolved.as_ref().and_then(|res| validate_binary(res).ok());

    RuntimeStatus {
        healthy,
        pid,
        binary: resolved.as_ref().map(|res| res.binary.clone()),
        source: resolved.map(|res| res.source),
        version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("c2-rt-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn config_binary_tem_prioridade() {
        let base = tmp("bin");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();

        let fake = base.join("llama-server-fake.exe");
        fs::write(&fake, b"fake").unwrap();

        let mut config = workspace.load_config().unwrap();
        config.model.binary = Some(fake.to_string_lossy().to_string());

        let resolved = resolve_binary(&workspace, &config).unwrap();
        assert_eq!(resolved.binary, fake);
        assert_eq!(resolved.source, "model.binary");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn binary_inexistente_na_config_e_erro_claro() {
        let base = tmp("missing");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        let mut config = workspace.load_config().unwrap();
        config.model.binary = Some(base.join("nao-existe.exe").to_string_lossy().to_string());

        let err = resolve_binary(&workspace, &config).unwrap_err();
        assert!(err.to_string().contains("inexistente"), "{err}");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn runtime_local_do_workspace_e_usado() {
        let base = tmp("local");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();

        let runtime_dir = workspace.c2_dir().join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let binary = runtime_dir.join(exe_name("llama-server"));
        fs::write(&binary, b"fake").unwrap();

        let config = workspace.load_config().unwrap();
        let resolved = resolve_binary(&workspace, &config).unwrap();
        assert_eq!(resolved.binary, binary);
        assert!(resolved.source.contains("workspace"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn health_responde_200() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
            }
        });

        let mut config = Config::default();
        config.model.host = "127.0.0.1".to_string();
        config.model.port = port;
        assert!(health(&config));

        config.model.port = port.wrapping_add(1).max(1);
        assert!(!health(&config));
    }

    #[cfg(windows)]
    #[test]
    fn stop_encerra_processo_registrado() {
        let base = tmp("stop");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();

        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 > nul"])
            .spawn()
            .unwrap();
        let pid = child.id();
        fs::write(pid_file(&workspace), pid.to_string()).unwrap();

        assert!(pid_alive(pid));
        assert!(stop(&workspace).unwrap());
        assert!(!pid_alive(pid), "processo deveria ter sido encerrado");
        let _ = child.wait();
        assert!(!pid_file(&workspace).exists());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn binario_inexistente_falha_na_validacao() {
        let resolved = Resolved {
            binary: PathBuf::from("c2-binario-que-nao-existe-xyz"),
            dll_dirs: vec![],
            source: "teste".to_string(),
        };
        let err = validate_binary(&resolved).unwrap_err();
        assert!(
            err.to_string().contains("nao foi possivel executar"),
            "{err}"
        );
    }
}
