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

use serde::{Deserialize, Serialize};
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
    /// Ultima linha de erro do log do llama-server.
    pub last_error: Option<String>,
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
        let mut comando = Command::new("tasklist");
        comando.args(["/FI", &format!("PID eq {pid}"), "/NH"]);
        let output = crate::process::output(comando);
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        let mut comando = Command::new("kill");
        comando.args(["-0", &pid.to_string()]);
        crate::process::status(comando)
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
    // Sem ninguem escutando, nem tenta HTTP: no Windows um connect para porta
    // fechada pode levar ~2 s (filtro/antivirus), travando o boot.
    if !porta_em_uso(&config.model.host, config.model.port) {
        return false;
    }
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

    let local = workspace
        .c2_dir()
        .join("runtime")
        .join(exe_name("llama-server"));
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
    let parent = dir
        .join("..")
        .join("runtime")
        .join(exe_name("llama-server"));
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
#[derive(Debug, Serialize, Deserialize)]
struct VersionCache {
    binary: String,
    mtime_secs: u64,
    version: String,
}

fn version_cache_path(workspace: &Workspace) -> PathBuf {
    workspace.c2_dir().join("runtime-version.json")
}

fn binary_mtime_secs(binary: &Path) -> Option<u64> {
    let mtime = fs::metadata(binary).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duracao| duracao.as_secs())
}

/// Le a versao do cache sem executar o binario (boot rapido).
fn cached_version(workspace: &Workspace, resolved: &Resolved) -> Option<String> {
    let texto = fs::read_to_string(version_cache_path(workspace)).ok()?;
    let cache: VersionCache = serde_json::from_str(&texto).ok()?;
    let mtime = binary_mtime_secs(&resolved.binary)?;
    if cache.binary == resolved.binary.display().to_string() && cache.mtime_secs == mtime {
        Some(cache.version)
    } else {
        None
    }
}

/// Executa `--version` uma vez e guarda no workspace para os proximos status.
fn remember_version(workspace: &Workspace, resolved: &Resolved) {
    let Ok(version) = validate_binary(resolved) else {
        return;
    };
    let Some(mtime_secs) = binary_mtime_secs(&resolved.binary) else {
        return;
    };
    let cache = VersionCache {
        binary: resolved.binary.display().to_string(),
        mtime_secs,
        version,
    };
    let caminho = version_cache_path(workspace);
    if let Some(pai) = caminho.parent() {
        let _ = fs::create_dir_all(pai);
    }
    let _ = fs::write(caminho, serde_json::to_string(&cache).unwrap_or_default());
}

pub fn validate_binary(resolved: &Resolved) -> Result<String> {
    let mut command = Command::new(&resolved.binary);
    command.arg("--version");
    if let Some(path) = path_with_dlls(&resolved.dll_dirs) {
        command.env("PATH", path);
    }
    let output = crate::process::output(command).map_err(|err| {
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

/// Valida o arquivo antes de gastar tempo subindo o servidor (c2-e4t6).
fn validar_modelo(model: &str) -> Result<()> {
    if !Path::new(model).is_file() {
        return Err(C2Error::InvalidConfig(format!(
            "modelo GGUF nao encontrado: {model}\n  dica: c2 config set model.path \"<caminho do .gguf>\""
        )));
    }
    crate::gguf::read_info(Path::new(model)).map_err(|err| {
        C2Error::InvalidConfig(format!(
            "o arquivo nao parece um GGUF valido ({model}): {err}\n  dica: baixe de novo o .gguf (o download pode ter sido interrompido)"
        ))
    })?;
    Ok(())
}

/// Alguem ja escuta nesta porta? (evita subir e falhar em silencio)
pub fn porta_em_uso(host: &str, port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let Ok(enderecos) = (host, port).to_socket_addrs() else {
        return false;
    };
    for endereco in enderecos {
        if TcpStream::connect_timeout(&endereco, Duration::from_millis(150)).is_ok() {
            return true;
        }
    }
    false
}

/// Pede ao sistema uma porta livre no host (fecha o listener em seguida).
pub fn porta_livre(host: &str) -> Option<u16> {
    std::net::TcpListener::bind((host, 0))
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

/// `ensure`, mas troca para uma porta livre se a configurada estiver ocupada.
/// Atualiza o config do workspace para os clientes usarem a mesma porta.
pub fn ensure_auto(workspace: &Workspace, config: &mut Config) -> Result<Ensure> {
    if crate::config::usa_runtime_local(config)
        && !health(config)
        && porta_em_uso(&config.model.host, config.model.port)
    {
        if let Some(porta) = porta_livre(&config.model.host) {
            config.model.port = porta;
            workspace.save_config(config)?;
        }
    }
    ensure(workspace, config)
}

/// Traduz o log do llama-server em uma mensagem acionavel.
fn explicar_falha(err_path: &Path, tail: &str) -> String {
    let minusculo = tail.to_ascii_lowercase();
    let falta_memoria = [
        "out of memory",
        "failed to allocate",
        "cuda error",
        "vulkan error",
        "ggml_backend_vk",
        "ggml_backend_cuda",
        "oom",
    ]
    .iter()
    .any(|marcador| minusculo.contains(marcador));

    if falta_memoria {
        return format!(
            "falta de memoria ao carregar o modelo: reduza model.ctx_size e model.gpu_layers, \
             ou feche outros programas que usam a GPU/RAM\n  log: {}\n{tail}",
            err_path.display()
        );
    }
    format!(
        "llama-server nao respondeu: o processo pode ter morrido ao carregar. \
         Verifique o log \n  log: {}\n{tail}",
        err_path.display()
    )
}

/// Ultimo erro registrado pelo runtime (para `c2 status`).
pub fn last_error(workspace: &Workspace) -> Option<String> {
    let path = workspace.logs_dir().join("llama.err.log");
    let texto = fs::read_to_string(path).ok()?;
    let linha = texto
        .lines()
        .rev()
        .map(str::trim)
        .find(|linha| !linha.is_empty() && !linha.starts_with("llama-server: server"))?;
    Some(linha.chars().take(200).collect())
}

/// Sobe o servidor, espera o health check e devolve o PID.
pub fn start(workspace: &Workspace, config: &Config, resolved: &Resolved) -> Result<u32> {
    let model = config.model.path.as_deref().ok_or_else(|| {
        C2Error::InvalidConfig(
            "nenhum modelo configurado (model.path vazio)\n  dica: c2 config set model.path \"<caminho do .gguf>\" ou use a tela Modelo".to_string(),
        )
    })?;
    validar_modelo(model)?;
    if porta_em_uso(&config.model.host, config.model.port) {
        return Err(C2Error::InvalidConfig(format!(
            "porta {} ja esta em uso em {}\n  dica: feche o processo que ocupa a porta ou rode: c2 config set model.port {}",
            config.model.port,
            config.model.host,
            config.model.port.saturating_add(1)
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

    let child = crate::process::spawn(command).map_err(|err| {
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
        if !pid_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = stop(workspace);
    let tail = fs::read_to_string(&err_path).unwrap_or_default();
    let tail: String = tail.lines().rev().take(8).collect::<Vec<_>>().join("\n");
    Err(C2Error::Llm(explicar_falha(&err_path, &tail)))
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
                remember_version(workspace, &resolved);
                return Ok(Ensure::Started {
                    attempts: attempt + 1,
                });
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
    let status = crate::process::status({
        let mut comando = Command::new("taskkill");
        comando
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        comando
    });
    #[cfg(not(windows))]
    let status = {
        let mut comando = Command::new("kill");
        comando.args(["-TERM", &pid.to_string()]);
        crate::process::status(comando)
    };

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
            let _ = crate::process::status({
                let mut comando = Command::new("taskkill");
                comando
                    .args(["/PID", &pid.to_string(), "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                comando
            });
        }
        #[cfg(not(windows))]
        {
            let mut comando = Command::new("kill");
            comando.args(["-9", &pid.to_string()]);
            let _ = crate::process::status(comando);
        }
    }

    let _ = fs::remove_file(pid_file(workspace));
    Ok(killeed)
}

/// Resumo para `c2 status`.
pub fn status(workspace: &Workspace, config: &Config) -> RuntimeStatus {
    // Provedor externo (nuvem/BYO): quem gerencia o servidor e outro software.
    // Nao sonde health (o base_url cru pode dar falso positivo) nem procure binario.
    if !crate::config::usa_runtime_local(config) {
        return RuntimeStatus {
            healthy: false,
            pid: None,
            binary: None,
            source: Some("externo".to_string()),
            version: None,
            last_error: None,
        };
    }
    let t = Instant::now();
    let healthy = health(config);
    let health_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    let pid = current_pid(workspace);
    let pid_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    let resolved = resolve_binary(workspace, config).ok();
    let resolve_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    // Nunca executa --version no status: no cold start isso carrega ~100 MB de
    // runtime/DLLs e travava o boot. Usa o cache gravado ao iniciar o engine.
    let version = resolved
        .as_ref()
        .and_then(|res| cached_version(workspace, res));
    let cache_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    let ultimo_erro = last_error(workspace);
    let log_ms = t.elapsed().as_millis() as u64;
    if std::env::var_os("C2_PERF").is_some() {
        eprintln!(
            "[runtime] health={health_ms}ms pid={pid_ms}ms resolve={resolve_ms}ms cache={cache_ms}ms log={log_ms}ms"
        );
    }

    RuntimeStatus {
        healthy,
        pid,
        binary: resolved.as_ref().map(|res| res.binary.clone()),
        source: resolved.map(|res| res.source),
        version,
        last_error: ultimo_erro,
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

#[cfg(test)]
mod erro_tests {
    use super::*;
    use std::net::TcpListener;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("c2-err-{tag}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn falta_de_memoria_tem_mensagem_propria() {
        let mensagem = explicar_falha(
            Path::new(".c2/logs/llama.err.log"),
            "ggml_backend_cuda: failed to allocate 4.00 MiB",
        );
        assert!(mensagem.contains("falta de memoria"), "{mensagem}");
        assert!(mensagem.contains("ctx_size"), "{mensagem}");
    }

    #[test]
    fn modelo_ausente_cita_como_configurar() {
        let base = tmp("sem-modelo");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        let config = workspace.load_config().unwrap();
        let resolvido = Resolved {
            binary: base.join("llama-server-inexistente"),
            dll_dirs: Vec::new(),
            source: "teste".to_string(),
        };

        let erro = start(&workspace, &config, &resolvido)
            .unwrap_err()
            .to_string();
        assert!(erro.contains("c2 config set model.path"), "{erro}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn modelo_corrompido_e_recusado_com_dica() {
        let base = tmp("corrompido");
        let modelo = base.join("modelo.gguf");
        std::fs::write(&modelo, b"nao sou gguf").unwrap();

        let erro = validar_modelo(&modelo.display().to_string())
            .unwrap_err()
            .to_string();
        assert!(erro.contains("GGUF"), "{erro}");
        assert!(erro.contains("baixe de novo"), "{erro}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn porta_ocupada_e_detectada() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = listener.local_addr().unwrap().port();
        assert!(porta_em_uso("127.0.0.1", porta));
        drop(listener);

        // porta sem listener nao acusa uso
        let livre = TcpListener::bind("127.0.0.1:0").unwrap();
        let livre_porta = livre.local_addr().unwrap().port();
        drop(livre);
        assert!(!porta_em_uso("127.0.0.1", livre_porta));
    }

    #[test]
    fn status_reflete_ultimo_erro_do_runtime() {
        let base = tmp("status");
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        let logs = workspace.logs_dir();
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("llama.err.log"), "load_tensors: out of memory\n").unwrap();

        let erro = last_error(&workspace).unwrap();
        assert!(erro.contains("out of memory"), "{erro}");
        std::fs::remove_dir_all(&base).ok();
    }
}
