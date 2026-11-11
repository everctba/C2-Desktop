//! Instancia unica do C2 e foco da janela existente.
//!
//! Windows e Linux usam o mesmo protocolo TCP loopback local com token. O
//! arquivo de instancia aponta para a porta/token; o CLI consulta antes de
//! spawnar outra janela. O servidor so aceita 127.0.0.1 e o token do usuario.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{C2Error, Result};

const BASE_PORT: u16 = 47000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub port: u16,
    pub token: String,
}

#[derive(Debug, Deserialize)]
struct Pedido {
    token: String,
    action: String,
    root: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Resposta {
    ok: bool,
    error: Option<String>,
}

/// Diretorio/arquivo de instancia. Usa os mesmos caminhos no app e no CLI.
pub fn instance_file() -> PathBuf {
    let base = if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("dev.c2.desktop")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("dev.c2.desktop")
    } else {
        std::env::temp_dir()
    };
    base.join("instance.json")
}

fn token() -> String {
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut h = Sha256::new();
    h.update(std::process::id().to_le_bytes());
    h.update(agora.to_le_bytes());
    if let Ok(user) = std::env::var("USERNAME").or_else(|_| std::env::var("USER")) {
        h.update(user.as_bytes());
    }
    format!("{:x}", h.finalize())
}

/// Porta estavel por usuario, para duas contas no mesmo host nao colidirem.
fn base_port() -> u16 {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "c2".to_string());
    let mut h = Sha256::new();
    h.update(user.as_bytes());
    let digest = h.finalize();
    let offset = u16::from(digest[0]) * 8 + u16::from(digest[1]) % 8;
    BASE_PORT + (offset % 1500)
}

fn escrever_info(info: &InstanceInfo) -> Result<()> {
    let path = instance_file();
    if let Some(pai) = path.parent() {
        fs::create_dir_all(pai).map_err(|e| C2Error::io(pai, e))?;
    }
    let tmp = path.with_extension("json.tmp");
    let texto = serde_json::to_string(info)?;
    fs::write(&tmp, texto).map_err(|e| C2Error::io(&tmp, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &path).map_err(|e| C2Error::io(&path, e))
}

fn ler_info() -> Option<InstanceInfo> {
    let texto = fs::read_to_string(instance_file()).ok()?;
    serde_json::from_str(&texto).ok()
}

fn remover_info() {
    let _ = fs::remove_file(instance_file());
}

/// Envia `focus` para uma instancia viva. Retorna falso se nao existir.
pub fn try_focus(root: Option<&Path>) -> bool {
    let Some(info) = ler_info() else {
        return false;
    };
    let endereco: SocketAddr = match format!("127.0.0.1:{}", info.port).parse() {
        Ok(endereco) => endereco,
        Err(_) => {
            remover_info();
            return false;
        }
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&endereco, Duration::from_millis(500)) else {
        remover_info();
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
    let pedido = serde_json::json!({
        "token": info.token,
        "action": "focus",
        "root": root.map(|p| p.to_string_lossy().to_string()),
    });
    if writeln!(stream, "{pedido}").is_err() {
        return false;
    }
    let mut linha = String::new();
    let Ok(_) = BufReader::new(stream).read_line(&mut linha) else {
        return false;
    };
    serde_json::from_str::<Resposta>(&linha)
        .map(|r| r.ok)
        .unwrap_or(false)
}

/// Tenta se tornar a instancia primaria. Devolve None quando ja existe outra.
pub fn claim() -> Result<Option<Primary>> {
    if try_focus(None) {
        return Ok(None);
    }
    let endereco = SocketAddr::from(([127, 0, 0, 1], base_port()));
    match TcpListener::bind(endereco) {
        Ok(listener) => {
            let info = InstanceInfo {
                port: base_port(),
                token: token(),
            };
            escrever_info(&info)?;
            Ok(Some(Primary { listener, info }))
        }
        Err(_) => {
            // Outra instancia pode estar subindo e ainda nao escreveu o arquivo.
            for _ in 0..20 {
                if try_focus(None) {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(C2Error::InvalidConfig(format!(
                "porta local {} ocupada e nenhuma instancia C2 respondeu",
                base_port()
            )))
        }
    }
}

/// Instancia primaria; o listener fica em uma thread de foco.
pub struct Primary {
    listener: TcpListener,
    info: InstanceInfo,
}

impl Primary {
    pub fn port(&self) -> u16 {
        self.info.port
    }

    /// Inicia o servidor de foco. O callback recebe o root opcional.
    pub fn serve<F>(self, callback: F)
    where
        F: Fn(Option<String>) + Send + 'static,
    {
        let token = self.info.token.clone();
        let handle = std::thread::Builder::new()
            .name("c2-single-instance".to_string())
            .spawn(move || {
                for stream in self.listener.incoming() {
                    let Ok(mut stream) = stream else {
                        continue;
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
                    let mut linha = String::new();
                    if BufReader::new(&stream).read_line(&mut linha).is_err() {
                        continue;
                    }
                    let resposta = match serde_json::from_str::<Pedido>(&linha) {
                        Ok(pedido) if pedido.token == token && pedido.action == "focus" => {
                            callback(pedido.root);
                            Resposta {
                                ok: true,
                                error: None,
                            }
                        }
                        Ok(_) => Resposta {
                            ok: false,
                            error: Some("token ou acao invalida".to_string()),
                        },
                        Err(erro) => Resposta {
                            ok: false,
                            error: Some(format!("json invalido: {erro}")),
                        },
                    };
                    let _ = writeln!(
                        stream,
                        "{}",
                        serde_json::to_string(&resposta).unwrap_or_default()
                    );
                }
            });
        if handle.is_err() {
            eprintln!("[c2] nao foi possivel iniciar thread de instancia unica");
        }
    }
}

impl Drop for Primary {
    fn drop(&mut self) {
        remover_info();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instancia_primaria_recebe_foco() {
        let info = InstanceInfo {
            port: 0,
            token: "teste".to_string(),
        };
        let _ = info;
        let primario = claim().expect("claim");
        let Some(primario) = primario else {
            return;
        };
        let port = primario.port();
        primario.serve(|root| {
            assert_eq!(root.as_deref(), Some("C:/projeto"));
        });
        let endereco: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut s = TcpStream::connect(endereco).expect("conectar");
        let info = ler_info().expect("info");
        let pedido = serde_json::json!({
            "token": info.token,
            "action": "focus",
            "root": "C:/projeto",
        });
        writeln!(s, "{pedido}").unwrap();
        let mut resposta = String::new();
        BufReader::new(s).read_line(&mut resposta).unwrap();
        assert!(resposta.contains("\"ok\":true"));
    }
}
