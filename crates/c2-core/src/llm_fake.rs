//! Servidor HTTP falso do `llama-server` para testes offline (bead `c2-e5t7`).
//!
//! Cobre os cinco cenarios do aceite: resposta valida, invalida, vazia, lenta
//! (timeout) e queda no meio da conexao.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

/// O que o servidor falso responde em cada conexao (em sequencia).
#[derive(Clone, Debug)]
pub enum Reply {
    /// 200 com o corpo JSON informado.
    Ok(Value),
    /// 200 com JSON invalido (texto cru).
    Raw(String),
    /// Status HTTP de erro com mensagem.
    Status(u16, String),
    /// Espera antes de responder (testa timeout do cliente).
    Slow(Duration, Value),
    /// Fecha a conexao sem responder (queda no meio).
    Drop,
}

/// Servidor falso escutando em uma porta livre.
pub struct FakeServer {
    pub port: u16,
    pub calls: Arc<AtomicUsize>,
}

impl FakeServer {
    pub fn chat_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1/chat/completions", self.port)
    }
}

/// Sobe o servidor em background respondendo `replies` na ordem.
pub fn start(replies: Vec<Reply>) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("porta livre para o servidor falso");
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&calls);

    std::thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buffer = [0u8; 65536];
            let lidos = stream.read(&mut buffer).unwrap_or(0);
            contador.fetch_add(1, Ordering::SeqCst);
            if lidos == 0 {
                continue;
            }

            // Consome o corpo restante (Content-Length) para o cliente nao travar.
            let cabecalho = String::from_utf8_lossy(&buffer[..lidos]).to_string();
            let mut corpo = cabecalho.split_once("\r\n\r\n").map(|(_, b)| b.len()).unwrap_or(0);
            let esperado = cabecalho
                .lines()
                .find(|linha| linha.to_ascii_lowercase().starts_with("content-length:"))
                .and_then(|linha| linha.split(':').nth(1))
                .and_then(|valor| valor.trim().parse::<usize>().ok())
                .unwrap_or(0);
            while corpo < esperado {
                let mut resto = vec![0u8; 8192];
                let n = stream.read(&mut resto).unwrap_or(0);
                if n == 0 {
                    break;
                }
                corpo += n;
            }

            match reply {
                Reply::Drop => {
                    // fecha sem escrever nada
                    drop(stream);
                }
                Reply::Slow(delay, valor) => {
                    std::thread::sleep(delay);
                    responder(&mut stream, 200, &valor.to_string());
                }
                Reply::Status(codigo, mensagem) => {
                    responder(&mut stream, codigo, &mensagem);
                }
                Reply::Raw(texto) => {
                    responder(&mut stream, 200, &texto);
                }
                Reply::Ok(valor) => {
                    responder(&mut stream, 200, &valor.to_string());
                }
            }
        }
    });

    FakeServer { port, calls }
}

fn responder(stream: &mut std::net::TcpStream, status: u16, corpo: &str) {
    let motivo = if status == 200 { "OK" } else { "ERRO" };
    let resposta = format!(
        "HTTP/1.1 {status} {motivo}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = stream.write_all(resposta.as_bytes());
    let _ = stream.flush();
}

/// Payload de completion valido do formato OpenAI/llama-server.
pub fn completion(content: &str, finish: &str) -> Value {
    serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": content }, "finish_reason": finish }],
        "usage": { "prompt_tokens": 11, "completion_tokens": 22 }
    })
}

/// Semantic valido ja em JSON (para respostas boas).
pub fn semantic_json() -> &'static str {
    r#"{"what":"w","why":"y","goal":"g","summary":"s","responsibility":"r"}"#
}
