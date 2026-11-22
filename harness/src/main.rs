//! C2 Harness (c2h): CLI minimo para contexto, blocos e anotacoes.
//!
//! O harness e dono de block_id, hashes e timestamps. A IA e usada apenas para
//! gerar conteudo; nada e aplicado sem validacao.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tree_sitter::{Node, Parser as TsParser};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u32 = 1;
const CONTRACT_VERSION: u32 = 1;

#[derive(Parser, Debug)]
#[command(
    name = "c2h",
    version,
    about = "C2 Harness: contexto e anotacoes por bloco"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    /// Timeline de debug em .c2h/logs
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Abre o modo interativo (padrao quando nao ha comando)
    Chat,
    /// Diagnostico do harness e do projeto
    Doctor,
    /// Mostra os blocos extraidos de um arquivo
    Blocks { file: PathBuf },
    /// Cria/atualiza o sidecar de blocos de um arquivo
    Annotate {
        file: PathBuf,
        /// Grava sem alterar (dry-run)
        #[arg(long)]
        dry_run: bool,
        /// Usa a IA para preencher o W5H2 dos blocos
        #[arg(long)]
        ai: bool,
    },
    /// Valida sidecar contra o codigo atual
    Validate { file: Option<PathBuf> },
    /// Mostra o estado dos sidecars do projeto
    Status,
    /// Mostra o contexto compacto que seria enviado a IA
    Context {
        file: PathBuf,
        #[arg(long, default_value_t = 12000)]
        max_chars: usize,
    },
    /// Envia um prompt para o provider OpenAI-compatible
    Ask {
        prompt: String,
        #[arg(long)]
        system: Option<String>,
        /// Inclui o contexto dos blocos deste arquivo e exige contrato JSON
        #[arg(long)]
        file: Option<PathBuf>,
        /// Aplica o patch apos validar
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value_t = 3)]
        max_repairs: usize,
    },
    /// Valida um arquivo de resposta contra o contrato e o sidecar atual
    ValidateResponse { response: PathBuf },
    /// Aplica um arquivo de resposta (com dry-run opcional)
    Apply {
        response: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Mostra os ultimos eventos da timeline de debug
    Logs {
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// Fuzzing deterministico do parser/validator de contrato
    Fuzz {
        #[arg(long, default_value_t = 500)]
        iters: usize,
    },
    /// Mostra a configuracao efetiva (sem segredos)
    Config,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct W5h2 {
    #[serde(default)]
    what: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    when: String,
    #[serde(default, rename = "where")]
    where_: String,
    #[serde(default)]
    who: String,
    #[serde(default)]
    how: String,
    #[serde(default)]
    how_much: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Block {
    id: String,
    kind: String,
    name: String,
    start_line: usize,
    end_line: usize,
    semantic_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SidecarBlock {
    id: String,
    kind: String,
    name: String,
    #[serde(default)]
    start_line: usize,
    #[serde(default)]
    end_line: usize,
    created_at: String,
    updated_at: String,
    semantic_hash: String,
    #[serde(default)]
    w5h2: W5h2,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sidecar {
    schema_version: u32,
    contract_version: u32,
    file: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    source_hash: String,
    #[serde(default)]
    generated_by: String,
    #[serde(default)]
    blocks: Vec<SidecarBlock>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResponseEnvelope {
    contract_version: u32,
    changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Change {
    file: String,
    block_id: String,
    action: String,
    #[serde(default)]
    new_code: String,
    #[serde(default)]
    w5h2: W5h2,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteEnvelope {
    path: String,
    content: String,
    #[serde(default)]
    imports: Vec<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    w5h2: W5h2,
}

#[derive(Debug, Clone, Serialize)]
struct Violation {
    code: String,
    severity: String,
    block_id: Option<String>,
    msg: String,
    fix: String,
}

fn safe_rel(file: &str) -> bool {
    let path = Path::new(file);
    !path.is_absolute()
        && !file.contains('\\')
        && !path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
}

fn contract_system() -> &'static str {
    r#"Voce e um agente de codigo do C2 Harness.

Responda APENAS JSON valido, sem markdown, no formato:
{"contract_version":1,"changes":[{"file":"...","block_id":"...","action":"update|create|delete","new_code":"...","w5h2":{"what":"...","why":"...","when":"...","where":"...","who":"...","how":"...","how_much":"..."},"evidence":["arquivo:linha"]}]}

Regras:
- block_id deve existir no sidecar para update/delete;
- new_code obrigatorio para create/update;
- evidence e OBRIGATORIA em toda mudanca, no formato arquivo:linha real;
- w5h2 deve ser curto e baseado em evidencia;
- nao escreva created_at/updated_at/timestamp;
- nao invente arquivo, block_id ou evidencia;
- nao altere testes para passar;
- nada de comandos shell;
- use unknown quando nao souber."#
}

fn extrair_json(texto: &str) -> Result<ResponseEnvelope, String> {
    let limpo = texto
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let inicio = limpo
        .find('{')
        .ok_or_else(|| "resposta sem JSON".to_string())?;
    let fim = limpo
        .rfind('}')
        .ok_or_else(|| "JSON incompleto".to_string())?;
    serde_json::from_str(&limpo[inicio..=fim]).map_err(|e| format!("JSON invalido: {e}"))
}

fn validar_resposta(raiz: &Path, resposta: &ResponseEnvelope) -> Vec<Violation> {
    let mut v = Vec::new();
    let mut erro = |code: &str, block: Option<&str>, msg: &str, fix: &str| {
        v.push(Violation {
            code: code.to_string(),
            severity: "error".to_string(),
            block_id: block.map(|s| s.to_string()),
            msg: msg.to_string(),
            fix: fix.to_string(),
        });
    };
    if resposta.contract_version != CONTRACT_VERSION {
        erro(
            "E-SCHEMA-01",
            None,
            "contract_version incompativel",
            "use contract_version 1",
        );
    }
    if resposta.changes.is_empty() {
        erro(
            "E-SCHEMA-02",
            None,
            "changes vazio",
            "inclua pelo menos uma mudanca",
        );
    }
    for change in &resposta.changes {
        if !safe_rel(&change.file) {
            erro(
                "E-PATH-01",
                Some(&change.block_id),
                "caminho invalido",
                "use caminho relativo dentro do projeto",
            );
            continue;
        }
        let file = raiz.join(&change.file);
        if !file.is_file() && change.action != "create" {
            erro(
                "E-FILE-01",
                Some(&change.block_id),
                "arquivo nao existe",
                "corrija o caminho ou use action=create",
            );
            continue;
        }
        if !matches!(change.action.as_str(), "update" | "create" | "delete") {
            erro(
                "E-ACTION-01",
                Some(&change.block_id),
                "action invalida",
                "use update/create/delete",
            );
        }
        if change.action != "delete" && change.new_code.trim().is_empty() {
            erro(
                "E-CODE-01",
                Some(&change.block_id),
                "new_code vazio",
                "retorne o codigo do bloco",
            );
        }
        if change.evidence.is_empty() {
            erro(
                "E-EVID-00",
                Some(&change.block_id),
                "evidencia obrigatoria",
                "cite arquivo:linha de onde veio a decisao",
            );
        }
        if change.action != "create" {
            let side = ler_sidecar(Path::new(&change.file));
            let existe = side
                .as_ref()
                .map(|s| s.blocks.iter().any(|b| b.id == change.block_id))
                .unwrap_or(false);
            if !existe {
                erro(
                    "E-BLOCK-01",
                    Some(&change.block_id),
                    "block_id desconhecido no sidecar",
                    "use um block_id retornado por c2h blocks/annotate",
                );
            }
        }
        if change.w5h2.what.trim().is_empty() && change.w5h2.why.trim().is_empty() {
            erro(
                "E-W5H2-01",
                Some(&change.block_id),
                "w5h2 sem what/why",
                "preencha what e why (ou unknown com evidencia)",
            );
        }
        for ev in &change.evidence {
            let mut partes = ev.splitn(2, ':');
            let caminho = partes.next().unwrap_or(ev);
            let linha = partes.next();
            let arquivo_ev = raiz.join(caminho);
            if !arquivo_ev.exists() {
                erro(
                    "E-EVID-01",
                    Some(&change.block_id),
                    "evidencia nao existe",
                    "use arquivo:linha existente",
                );
                continue;
            }
            if let Some(numero) = linha {
                let Ok(numero) = numero.parse::<usize>() else {
                    erro(
                        "E-EVID-02",
                        Some(&change.block_id),
                        "linha da evidencia nao e numerica",
                        "use arquivo:linha com numero real",
                    );
                    continue;
                };
                let total = fs::read_to_string(&arquivo_ev)
                    .map(|t| t.lines().count())
                    .unwrap_or(0);
                if numero == 0 || numero > total {
                    erro(
                        "E-EVID-02",
                        Some(&change.block_id),
                        "linha da evidencia fora do arquivo",
                        "use uma linha existente",
                    );
                }
            }
        }
    }
    v
}

fn montar_contexto(file: &Path) -> Result<String, String> {
    let (_, blocos) = carregar_arquivo(file)?;
    let side = ler_sidecar(file);
    let mut texto = format!("ARQUIVO ALVO: {}\n", rel_file(file));
    if let Some(side) = &side {
        for b in &side.blocks {
            texto.push_str(&format!(
                "\nBLOCO: {}\nwhat: {}\nwhy: {}\nupdated_at: {}\nsemantic_hash: {}\n",
                b.id, b.w5h2.what, b.w5h2.why, b.updated_at, b.semantic_hash
            ));
        }
    } else {
        for b in &blocos {
            texto.push_str(&format!(
                "\nBLOCO: {} (linhas {}-{})\n",
                b.id, b.start_line, b.end_line
            ));
        }
    }
    if let Ok(saida) = std::process::Command::new("git")
        .args(["diff", "--", &rel_file(file)])
        .output()
    {
        if saida.status.success() {
            let diff = String::from_utf8_lossy(&saida.stdout);
            if !diff.trim().is_empty() {
                texto.push_str("\nDIFF DESDE O ULTIMO COMMIT:\n");
                texto.push_str(&diff);
            }
        }
    }
    Ok(texto)
}

struct Spinner {
    ativo: std::sync::Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    fn novo(mensagem: &str) -> Self {
        let ativo = std::sync::Arc::new(AtomicBool::new(true));
        let handle = if std::io::stderr().is_terminal() {
            let ativo_thread = ativo.clone();
            let mensagem = mensagem.to_string();
            Some(std::thread::spawn(move || {
                let frames = ["|", "/", "-", "\\"];
                let inicio = Instant::now();
                let mut i = 0usize;
                while ativo_thread.load(Ordering::Relaxed) {
                    eprint!(
                        "\r{} {} {}s",
                        frames[i % frames.len()],
                        mensagem,
                        inicio.elapsed().as_secs()
                    );
                    let _ = std::io::stderr().flush();
                    i += 1;
                    std::thread::sleep(Duration::from_millis(120));
                }
                eprint!("\r{}\r", " ".repeat(72));
                let _ = std::io::stderr().flush();
            }))
        } else {
            None
        };
        Self { ativo, handle }
    }

    fn parar(&mut self) {
        self.ativo.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.parar();
    }
}

static MOCK_TOOL_USED: AtomicBool = AtomicBool::new(false);

fn chamar_provider(system: &str, user: &str) -> Result<String, String> {
    let mensagens = vec![
        serde_json::json!({"role": "system", "content": system}),
        serde_json::json!({"role": "user", "content": user}),
    ];
    chamar_provider_mensagens(system, &mensagens)
}

fn chamar_provider_mensagens(
    _system: &str,
    mensagens: &[serde_json::Value],
) -> Result<String, String> {
    let msg = chamar_api(mensagens, None)?;
    msg.get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "resposta sem content".to_string())
}

fn chamar_api(
    mensagens: &[serde_json::Value],
    tools: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    if tools.is_some() {
        if let Ok(mock) = env::var("C2H_MOCK_TOOL_CALL") {
            if !MOCK_TOOL_USED.swap(true, Ordering::Relaxed) {
                let value: serde_json::Value =
                    serde_json::from_str(&mock).map_err(|e| format!("mock tool invalido: {e}"))?;
                let nome = value
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list_dir");
                let args = value
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                log_evento("provider_mock_tool", serde_json::json!({"name": nome}));
                return Ok(serde_json::json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "mock-call-1",
                        "type": "function",
                        "function": {"name": nome, "arguments": args}
                    }]
                }));
            }
        }
    }
    log_evento(
        "provider_request",
        serde_json::json!({"model": model(), "messages": mensagens.len(), "tools": tools.is_some()}),
    );
    if let Ok(mock) = env::var("C2H_MOCK_RESPONSE") {
        log_evento(
            "provider_mock",
            serde_json::json!({"response_chars": mock.len()}),
        );
        return Ok(serde_json::json!({"role":"assistant","content": mock}));
    }
    let key = api_key().ok_or_else(|| "C2H_API_KEY/DEEPSEEK_API_KEY ausente".to_string())?;
    let _spinner = Spinner::novo("IA pensando");
    let mut body = serde_json::json!({
        "model": model(),
        "messages": mensagens,
        "temperature": 0.2,
        "stream": false
    });
    if let Some(tools) = tools {
        body["tools"] = tools.clone();
        body["tool_choice"] = serde_json::json!("auto");
    }
    let url = format!("{}/chat/completions", base_url().trim_end_matches('/'));
    let resposta = ureq::post(&url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .set("x-opencode-session", &session_id())
        .timeout(std::time::Duration::from_secs(120))
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Status(codigo, resposta) => {
                let corpo = resposta.into_string().unwrap_or_default();
                format!(
                    "provider status {codigo}: {}",
                    corpo.chars().take(800).collect::<String>()
                )
            }
            outro => format!("provider: {outro}"),
        })?;
    let json: serde_json::Value = resposta.into_json().map_err(|e| format!("json: {e}"))?;
    let msg = json
        .pointer("/choices/0/message")
        .cloned()
        .ok_or_else(|| "resposta sem choices[0].message".to_string())?;
    let tamanho = msg
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.len())
        .unwrap_or(0);
    log_evento(
        "provider_response",
        serde_json::json!({"chars": tamanho, "tool_calls": msg.get("tool_calls").is_some()}),
    );
    Ok(msg)
}

fn ferramentas() -> serde_json::Value {
    serde_json::json!([
        {"type":"function","function":{"name":"list_dir","description":"Lista arquivos e diretorios do projeto.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"diretorio relativo; vazio para o atual"}}}}},
        {"type":"function","function":{"name":"read_file","description":"Le um arquivo de texto, com faixa de linhas opcional.","parameters":{"type":"object","properties":{"path":{"type":"string"},"start":{"type":"integer"},"end":{"type":"integer"}},"required":["path"]}}},
        {"type":"function","function":{"name":"find_text","description":"Busca texto nos arquivos do projeto.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}},
        {"type":"function","function":{"name":"write_file","description":"Cria um arquivo novo pequeno. O harness valida, mostra preview e pede confirmacao.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"summary":{"type":"string"}},"required":["path","content"]}}},
        {"type":"function","function":{"name":"edit_file","description":"Edita um bloco de um arquivo existente via block_id, com patch validado e test gate.","parameters":{"type":"object","properties":{"path":{"type":"string"},"block_id":{"type":"string"},"new_code":{"type":"string"},"what":{"type":"string"},"why":{"type":"string"}},"required":["path","block_id","new_code"]}}},
        {"type":"function","function":{"name":"new_component","description":"Cria scaffold HTML/CSS/JS de um componente.","parameters":{"type":"object","properties":{"name":{"type":"string"},"html":{"type":"boolean"},"css":{"type":"boolean"},"js":{"type":"boolean"}},"required":["name"]}}},
        {"type":"function","function":{"name":"move_file","description":"Move um arquivo com backup.","parameters":{"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}}},
        {"type":"function","function":{"name":"remove_file","description":"Remove um arquivo com backup.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}},
        {"type":"function","function":{"name":"run_test","description":"Roda o comando de teste configurado (C2H_TEST_CMD).","parameters":{"type":"object","properties":{}}}}
    ])
}

#[derive(Clone)]
struct Workspace {
    root: PathBuf,
    root_canon: PathBuf,
    cwd: PathBuf,
}

impl Workspace {
    fn novo() -> Result<Self, String> {
        let root = env::current_dir().map_err(|e| e.to_string())?;
        let root_canon = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            root_canon,
            cwd: PathBuf::new(),
        })
    }

    fn resolver(&self, entrada: &str, existir: bool) -> Result<PathBuf, String> {
        if entrada.trim().is_empty() {
            return Err("caminho vazio".to_string());
        }
        let bruto = Path::new(entrada);
        if bruto.is_absolute() || entrada.contains('\\') {
            return Err("use caminho relativo com /".to_string());
        }
        if bruto
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("path traversal bloqueado: ..".to_string());
        }
        let rel = self.cwd.join(bruto);
        let mut limpo = PathBuf::new();
        for c in rel.components() {
            match c {
                std::path::Component::Normal(v) => limpo.push(v),
                std::path::Component::CurDir => {}
                _ => return Err("caminho invalido".to_string()),
            }
        }
        let full = self.root.join(&limpo);
        if existir {
            let canon = fs::canonicalize(&full).map_err(|_| format!("nao existe: {entrada}"))?;
            if !canon.starts_with(&self.root_canon) {
                return Err("caminho fora do projeto".to_string());
            }
            Ok(canon)
        } else {
            let mut ancestral = full.parent().ok_or_else(|| "caminho sem pai".to_string())?;
            while !ancestral.exists() {
                ancestral = ancestral
                    .parent()
                    .ok_or_else(|| "caminho sem ancestral existente".to_string())?;
            }
            let ancestral_canon = fs::canonicalize(ancestral).map_err(|e| e.to_string())?;
            if !ancestral_canon.starts_with(&self.root_canon) {
                return Err("caminho fora do projeto".to_string());
            }
            Ok(full)
        }
    }

    fn rel(&self, full: &Path) -> String {
        full.strip_prefix(&self.root)
            .or_else(|_| full.strip_prefix(&self.root_canon))
            .unwrap_or(full)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn cwd_abs(&self) -> PathBuf {
        self.root.join(&self.cwd)
    }

    fn entrar(&mut self, entrada: &str) -> Result<(), String> {
        let texto = entrada.trim();
        if texto.is_empty() || texto == "/" || texto == "~" {
            self.cwd = PathBuf::new();
            return Ok(());
        }
        let mut rel = self.cwd.clone();
        for comp in Path::new(texto).components() {
            match comp {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    rel.pop();
                }
                std::path::Component::RootDir => rel.clear(),
                std::path::Component::Normal(v) => rel.push(v),
                _ => return Err("caminho invalido".to_string()),
            }
        }
        let alvo = self.root.join(&rel);
        let canon = fs::canonicalize(&alvo).map_err(|_| format!("nao existe: {entrada}"))?;
        if !canon.starts_with(&self.root_canon) || !canon.is_dir() {
            return Err("diretorio fora do projeto ou invalido".to_string());
        }
        self.cwd = canon
            .strip_prefix(&self.root_canon)
            .map_err(|_| "fora do projeto".to_string())?
            .to_path_buf();
        Ok(())
    }
}

fn listar_dir(ws: &Workspace, entrada: Option<&str>) -> Result<(), String> {
    let dir = match entrada {
        Some(e) => ws.resolver(e, true)?,
        None => ws.cwd_abs(),
    };
    let mut itens: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();
    itens.sort_by_key(|e| e.file_name());
    for item in itens {
        let nome = item.file_name().to_string_lossy().to_string();
        let meta = item.metadata().ok();
        if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
            println!("{nome}/");
        } else {
            let tam = meta.map(|m| m.len()).unwrap_or(0);
            println!("{nome}  {tam} B");
        }
    }
    Ok(())
}

fn ler_arquivo_chat(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let partes: Vec<&str> = entrada.split_whitespace().collect();
    if partes.is_empty() {
        return Err("uso: /read <arquivo> [inicio:fim]".to_string());
    }
    let path = ws.resolver(partes[0], true)?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.iter().take(2048).any(|b| *b == 0) {
        return Err("arquivo binario".to_string());
    }
    let texto = String::from_utf8_lossy(&bytes);
    let linhas: Vec<&str> = texto.lines().collect();
    let (ini, fim) = if let Some(faixa) = partes.get(1) {
        let mut it = faixa.split(':');
        let a = it
            .next()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        let b = it
            .next()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(linhas.len())
            .min(linhas.len());
        (a, b)
    } else {
        (1, linhas.len().min(400))
    };
    for (i, linha) in linhas
        .iter()
        .enumerate()
        .skip(ini.saturating_sub(1))
        .take(fim.saturating_sub(ini) + 1)
    {
        println!("{:>5}: {linha}", i + 1);
    }
    if linhas.len() > fim {
        println!("... ({} linhas no total)", linhas.len());
    }
    Ok(())
}

fn arvore(ws: &Workspace, max_depth: usize) -> Result<(), String> {
    fn andar(dir: &Path, prefixo: &str, nivel: usize, max: usize) {
        if nivel > max {
            return;
        }
        let Ok(entradas) = fs::read_dir(dir) else {
            return;
        };
        let mut itens: Vec<_> = entradas.filter_map(|e| e.ok()).collect();
        itens.sort_by_key(|e| e.file_name());
        for item in itens {
            let nome = item.file_name().to_string_lossy().to_string();
            if matches!(nome.as_str(), "target" | ".git" | "node_modules" | ".c2h") {
                continue;
            }
            let path = item.path();
            let is_dir = path.is_dir();
            println!("{prefixo}{nome}{}", if is_dir { "/" } else { "" });
            if is_dir && nivel < max {
                andar(&path, &format!("{prefixo}  "), nivel + 1, max);
            }
        }
    }
    andar(&ws.cwd_abs(), "", 0, max_depth.min(6));
    Ok(())
}

fn buscar_texto(ws: &Workspace, termo: &str) -> Result<(), String> {
    if termo.trim().is_empty() {
        return Err("uso: /find <texto>".to_string());
    }
    let termo_lower = termo.to_lowercase();
    let mut achados = 0usize;
    fn andar(ws: &Workspace, dir: &Path, termo: &str, achados: &mut usize) {
        let Ok(entradas) = fs::read_dir(dir) else {
            return;
        };
        for item in entradas.filter_map(|e| e.ok()) {
            if *achados >= 100 {
                return;
            }
            let nome = item.file_name().to_string_lossy().to_string();
            if matches!(nome.as_str(), "target" | ".git" | "node_modules" | ".c2h") {
                continue;
            }
            let path = item.path();
            if path.is_dir() {
                andar(ws, &path, termo, achados);
                continue;
            }
            let Ok(meta) = item.metadata() else {
                continue;
            };
            if meta.len() > 1_500_000 {
                continue;
            }
            let Ok(texto) = fs::read_to_string(&path) else {
                continue;
            };
            for (i, linha) in texto.lines().enumerate() {
                if linha.to_lowercase().contains(&termo.to_lowercase()) {
                    println!("{}:{}: {}", ws.rel(&path), i + 1, linha.trim());
                    *achados += 1;
                    if *achados >= 100 {
                        return;
                    }
                }
            }
        }
    }
    andar(ws, &ws.cwd_abs(), &termo_lower, &mut achados);
    println!("{achados} resultado(s)");
    Ok(())
}

fn importacoes(conteudo: &str, ext: &str) -> Vec<String> {
    let mut saida = Vec::new();
    for linha in conteudo.lines() {
        let t = linha.trim();
        match ext {
            "html" | "htm" => {
                for marcador in ["href=\"", "src=\""] {
                    if let Some(resto) = t.split(marcador).nth(1) {
                        if let Some(fim) = resto.find('"') {
                            saida.push(resto[..fim].to_string());
                        }
                    }
                }
            }
            "css" => {
                if let Some(resto) = t.split("@import").nth(1) {
                    if let Some(v) = resto.split('"').nth(1) {
                        saida.push(v.to_string());
                    }
                }
                if let Some(resto) = t.split("url(").nth(1) {
                    if let Some(v) = resto.split(')').next() {
                        saida.push(v.trim_matches(|c| c == '"' || c == '\'').to_string());
                    }
                }
            }
            "js" | "ts" | "tsx" => {
                if let Some(resto) = t.split(" from ").nth(1) {
                    if let Some(v) = resto.split('"').nth(1) {
                        saida.push(v.to_string());
                    }
                }
                if let Some(resto) = t.split("require(").nth(1) {
                    if let Some(v) = resto.split('"').nth(1) {
                        saida.push(v.to_string());
                    }
                }
            }
            "rs" => {
                if let Some(resto) = t.strip_prefix("mod ") {
                    if let Some(nome) = resto.trim_end_matches(';').split_whitespace().next() {
                        saida.push(format!("{nome}.rs"));
                    }
                }
            }
            _ => {}
        }
    }
    saida
}

fn cmd_imports(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let path = ws.resolver(entrada.trim(), true)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let texto = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let base = path.parent().unwrap_or(&ws.root);
    for imp in importacoes(&texto, &ext) {
        if imp.starts_with("http") || imp.starts_with('#') || imp.starts_with("data:") {
            continue;
        }
        let alvo = base.join(&imp);
        let status = if alvo.exists() { "ok" } else { "FALTANDO" };
        println!("{status}  {imp}");
    }
    Ok(())
}

fn confirmar(pergunta: &str) -> Result<bool, String> {
    use std::io::BufRead;
    print!("{pergunta} [s/N] ");
    let _ = std::io::stdout().flush();
    let mut linha = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut linha)
        .map_err(|e| e.to_string())?;
    Ok(matches!(
        linha.trim().to_lowercase().as_str(),
        "s" | "sim" | "y" | "yes"
    ))
}

fn escrever_atomico(path: &Path, texto: &str) -> Result<(), String> {
    if let Some(pai) = path.parent() {
        fs::create_dir_all(pai).map_err(|e| format!("criar diretorio: {e}"))?;
    }
    let tmp = path.with_extension(format!("c2h-{}.tmp", std::process::id()));
    fs::write(&tmp, texto).map_err(|e| format!("gravar tmp: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename: {e}")
    })
}

fn backup(ws: &Workspace, path: &Path) -> Result<PathBuf, String> {
    let nome = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("arquivo");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dir = ws.root.join(".c2h/backup");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let destino = dir.join(format!("{stamp}-{nome}"));
    fs::copy(path, &destino).map_err(|e| format!("backup: {e}"))?;
    Ok(destino)
}

fn contexto_vizinhanca(ws: &Workspace, path: &Path) -> String {
    let dir = path.parent().unwrap_or(&ws.root);
    let mut texto = format!(
        "ARQUIVO NOVO: {}\nDIRETORIO: {}\n",
        ws.rel(path),
        ws.rel(dir)
    );
    if let Ok(entradas) = fs::read_dir(dir) {
        texto.push_str("IRMAOS:\n");
        let mut nomes: Vec<String> = entradas
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        nomes.sort();
        for nome in nomes.into_iter().take(40) {
            texto.push_str(&format!("- {nome}\n"));
        }
    }
    let readme = ws.root.join("README.md");
    if let Ok(conteudo) = fs::read_to_string(readme) {
        texto.push_str("README (inicio):\n");
        texto.push_str(&conteudo.lines().take(40).collect::<Vec<_>>().join("\n"));
        texto.push('\n');
    }
    texto
}

fn validar_novo(
    ws: &Workspace,
    requisitado: &Path,
    envelope: &WriteEnvelope,
    instrucao: &str,
) -> Result<(), String> {
    if envelope.content.trim().is_empty() {
        return Err("conteudo vazio".to_string());
    }
    if envelope.content.len() > 200_000 {
        return Err("arquivo grande demais (max 200 KB)".to_string());
    }
    let linhas = envelope.content.lines().count();
    if linhas > 150 {
        println!(
            "aviso estrutural: {linhas} linhas; prefira <= 150 e divida em componentes/secoes"
        );
    }
    let ext = requisitado
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(
        ext.as_str(),
        "html" | "htm" | "css" | "js" | "ts" | "tsx" | "md" | "json" | "rs" | "py" | "txt"
    ) {
        return Err(format!("extensao nao permitida: {ext}"));
    }
    let instr = instrucao.to_lowercase();
    if (instr.contains("sem javascript") || instr.contains("sem js"))
        && (envelope.content.to_lowercase().contains("<script")
            || envelope.content.to_lowercase().contains("javascript:"))
    {
        return Err("o pedido era sem JavaScript".to_string());
    }
    if let Some(erro) = violacoes_politica(&envelope.content, instrucao)
        .into_iter()
        .next()
    {
        return Err(erro);
    }
    for imp in &envelope.imports {
        if imp.starts_with("http") {
            return Err(format!("import externo nao permitido: {imp}"));
        }
        let alvo = requisitado.parent().unwrap_or(&ws.root).join(imp);
        if !alvo.exists() {
            return Err(format!("import nao encontrado: {imp}"));
        }
    }
    let baixo = envelope.content.to_lowercase();
    for segredo in ["sk-", "api_key=", "password=", "secret="] {
        if baixo.contains(segredo) {
            return Err("possivel segredo no conteudo".to_string());
        }
    }
    if envelope.content.contains("http://") || envelope.content.contains("https://") {
        println!("aviso: conteudo contem URL externa");
    }
    if ws.rel(requisitado) != envelope.path.replace('\\', "/") {
        println!(
            "aviso: path do contrato difere do pedido; usando {}",
            ws.rel(requisitado)
        );
    }
    Ok(())
}

fn cmd_write(ws: &Workspace, rest: &str) -> Result<(), String> {
    let mut rest = rest.trim();
    let mut sim = false;
    let mut force = false;
    loop {
        if let Some(r) = rest.strip_prefix("--yes") {
            sim = true;
            rest = r.trim();
            continue;
        }
        if let Some(r) = rest.strip_prefix("--force") {
            force = true;
            rest = r.trim();
            continue;
        }
        break;
    }
    let (alvo_txt, instrucao) = match rest.split_once(char::is_whitespace) {
        Some((a, b)) => (a.trim(), b.trim().trim_matches('"')),
        None => (rest, ""),
    };
    if alvo_txt.is_empty() {
        return Err("uso: /write <arquivo> \"instrucao\"".to_string());
    }
    let alvo = ws.resolver(alvo_txt, false)?;
    if alvo.exists() && !force {
        return Err("arquivo ja existe; use /edit ou --force".to_string());
    }
    if alvo.exists() {
        backup(ws, &alvo)?;
    }
    let contexto = contexto_vizinhanca(ws, &alvo);
    let system = "Voce cria um arquivo pequeno e focado. Responda APENAS JSON: {\"path\":\"...\",\"content\":\"...\",\"imports\":[],\"summary\":\"...\",\"w5h2\":{\"what\":\"...\",\"why\":\"...\",\"how\":\"...\"}}. Prefira menos de 150 linhas e uma responsabilidade. Nao escreva timestamp.";
    let user = format!(
        "ARQUIVO: {}\nINSTRUCAO: {}\nCONTEXTO:\n{}",
        ws.rel(&alvo),
        instrucao,
        contexto
    );
    let resposta = chamar_provider(system, &user)?;
    let json = extrair_json_valor(&resposta)?;
    let envelope: WriteEnvelope =
        serde_json::from_value(json).map_err(|e| format!("contrato de criacao invalido: {e}"))?;
    validar_novo(ws, &alvo, &envelope, instrucao)?;
    revisar_requisitos(&envelope.content, instrucao)?;
    let linhas = envelope.content.lines().count();
    if linhas > 150 {
        if let Err(erro) = sugerir_divisao(&envelope.content, instrucao) {
            println!("aviso: sugestao de divisao falhou: {erro}");
        }
    }
    println!("\n--- preview {} ({} linhas) ---", ws.rel(&alvo), linhas);
    for (i, linha) in envelope.content.lines().take(200).enumerate() {
        println!("{:>4}: {linha}", i + 1);
    }
    if !sim && !confirmar("gravar?")? {
        println!("cancelado");
        return Ok(());
    }
    if alvo.exists() {
        fs::remove_file(&alvo).map_err(|e| format!("remover para sobrescrever: {e}"))?;
    }
    escrever_atomico(&alvo, &envelope.content)?;
    cmd_annotate(Path::new(&ws.rel(&alvo)), false)?;
    if precisa_anotacao(Path::new(&ws.rel(&alvo))) {
        if let Err(erro) = anotar_blocos_ia(Path::new(&ws.rel(&alvo))) {
            println!("aviso: annotation pass falhou: {erro}");
        }
    }
    if let Some(mut side) = ler_sidecar(Path::new(&ws.rel(&alvo))) {
        if let Some(primeiro) = side.blocks.first_mut() {
            primeiro.w5h2 = envelope.w5h2.clone();
        }
        escrever_sidecar(Path::new(&ws.rel(&alvo)), &side)?;
    }
    log_evento(
        "write_file",
        serde_json::json!({"path": ws.rel(&alvo), "lines": linhas}),
    );
    println!("criado: {} ({} linhas)", ws.rel(&alvo), linhas);
    Ok(())
}

fn cmd_new_component(ws: &Workspace, rest: &str) -> Result<(), String> {
    let mut partes = rest.split_whitespace();
    let nome = partes
        .next()
        .ok_or_else(|| "uso: /new component <Nome> [--html] [--css] [--js]".to_string())?;
    let flags: Vec<&str> = partes.collect();
    let slug = nome.to_lowercase().replace(' ', "-");
    let dir = ws.resolver(&format!("src/components/{slug}"), false)?;
    if dir.exists() {
        return Err(format!("componente ja existe: {}", ws.rel(&dir)));
    }
    let quer_html = flags.is_empty() || flags.contains(&"--html");
    let quer_css = flags.is_empty() || flags.contains(&"--css");
    let quer_js = flags.contains(&"--js");
    let mut criados = Vec::new();
    if quer_html {
        let link = if quer_css {
            format!("  <link rel=\"stylesheet\" href=\"{slug}.css\">\n")
        } else {
            String::new()
        };
        let html = format!("<!doctype html>\n<html lang=\"pt-BR\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{nome}</title>\n{link}</head>\n<body>\n  <section data-c2-block=\"{slug}\">\n    <h1>{nome}</h1>\n  </section>\n</body>\n</html>\n");
        let path = dir.join(format!("{slug}.html"));
        escrever_atomico(&path, &html)?;
        criados.push(path);
    }
    if quer_css {
        let css = format!("/* {nome} */\n.{slug} {{\n  display: block;\n}}\n");
        let path = dir.join(format!("{slug}.css"));
        escrever_atomico(&path, &css)?;
        criados.push(path);
    }
    if quer_js {
        let js = format!("// {nome}\n");
        let path = dir.join(format!("{slug}.js"));
        escrever_atomico(&path, &js)?;
        criados.push(path);
    }
    for path in &criados {
        let rel = ws.rel(path);
        cmd_annotate(Path::new(&rel), false)?;
        println!("criado: {rel}");
    }
    log_evento(
        "new_component",
        serde_json::json!({"name": nome, "files": criados.len()}),
    );
    Ok(())
}

fn cmd_rm(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let path = ws.resolver(entrada.trim(), true)?;
    if path.is_dir() {
        return Err("remocao de diretorio bloqueada".to_string());
    }
    if !confirmar(&format!("apagar {}?", ws.rel(&path)))? {
        println!("cancelado");
        return Ok(());
    }
    let backup_path = backup(ws, &path)?;
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    let rel = ws.rel(&path);
    let _ = fs::remove_file(sidecar_path(Path::new(&rel)));
    log_evento(
        "rm_file",
        serde_json::json!({"path": rel, "backup": backup_path.display().to_string()}),
    );
    println!("apagado: {rel} (backup: {})", backup_path.display());
    Ok(())
}

fn cmd_mv(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let mut it = entrada.split_whitespace();
    let origem = it
        .next()
        .ok_or_else(|| "uso: /mv <origem> <destino>".to_string())?;
    let destino = it
        .next()
        .ok_or_else(|| "uso: /mv <origem> <destino>".to_string())?;
    let origem_path = ws.resolver(origem, true)?;
    let destino_path = ws.resolver(destino, false)?;
    if destino_path.exists() {
        return Err("destino ja existe".to_string());
    }
    if !confirmar(&format!(
        "mover {} -> {}?",
        ws.rel(&origem_path),
        ws.rel(&destino_path)
    ))? {
        println!("cancelado");
        return Ok(());
    }
    backup(ws, &origem_path)?;
    if let Some(pai) = destino_path.parent() {
        fs::create_dir_all(pai).map_err(|e| e.to_string())?;
    }
    fs::rename(&origem_path, &destino_path).map_err(|e| e.to_string())?;
    let rel_origem = ws.rel(&origem_path);
    let rel_destino = ws.rel(&destino_path);
    let _ = fs::remove_file(sidecar_path(Path::new(&rel_origem)));
    cmd_annotate(Path::new(&rel_destino), false)?;
    log_evento(
        "mv_file",
        serde_json::json!({"from": rel_origem, "to": rel_destino}),
    );
    println!("movido: {rel_origem} -> {rel_destino}");
    Ok(())
}

fn cmd_preview(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let path = ws.resolver(entrada.trim(), true)?;
    if path.extension().and_then(|e| e.to_str()) != Some("html") {
        return Err("preview suporta apenas .html".to_string());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("abrir navegador: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        println!("file://{}", path.display());
    }
    println!("preview: {}", ws.rel(&path));
    Ok(())
}

fn cmd_edit(ws: &Workspace, entrada: &str) -> Result<(), String> {
    let (file, instrucao) = entrada
        .split_once(char::is_whitespace)
        .or_else(|| entrada.split_once('"'))
        .ok_or_else(|| "uso: /edit <arquivo> \"instrucao\"".to_string())?;
    let path = ws.resolver(file.trim(), true)?;
    let rel = ws.rel(&path);
    let vazio = ler_sidecar(Path::new(&rel))
        .map(|s| s.blocks.is_empty())
        .unwrap_or(true);
    if vazio {
        anotar_blocos_ia(Path::new(&rel))?;
    }
    println!("editando {} ...", rel);
    cmd_ask_contract(instrucao.trim_matches('"'), Path::new(&rel), 3)
}

fn texto_listar(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let entrada = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let dir = if entrada.trim().is_empty() {
        ws.cwd_abs()
    } else {
        ws.resolver(entrada, true)?
    };
    let mut saida = String::new();
    let mut itens: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .collect();
    itens.sort_by_key(|e| e.file_name());
    for item in itens {
        let nome = item.file_name().to_string_lossy().to_string();
        let is_dir = item.metadata().map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            saida.push_str(&format!("{nome}/\n"));
        } else {
            let rel = ws.rel(&item.path());
            let status = ler_sidecar(Path::new(&rel))
                .map(|s| format!(" [sidecar:{} {} blocos]", s.status, s.blocks.len()))
                .unwrap_or_default();
            saida.push_str(&format!("{nome}{status}\n"));
        }
    }
    Ok(saida)
}

fn texto_ler(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let path = ws.resolver(
        args.get("path").and_then(|v| v.as_str()).unwrap_or(""),
        true,
    )?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.iter().take(2048).any(|b| *b == 0) {
        return Err("arquivo binario".to_string());
    }
    let texto = String::from_utf8_lossy(&bytes);
    let linhas: Vec<&str> = texto.lines().collect();
    let ini = args
        .get("start")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1) as usize;
    let fim = args
        .get("end")
        .and_then(|v| v.as_u64())
        .unwrap_or(linhas.len() as u64)
        .min(linhas.len() as u64) as usize;
    let mut saida = String::new();
    for (i, linha) in linhas
        .iter()
        .enumerate()
        .skip(ini - 1)
        .take(fim.saturating_sub(ini) + 1)
    {
        saida.push_str(&format!("{:>5}: {linha}\n", i + 1));
    }
    if let Some(side) = ler_sidecar(Path::new(&ws.rel(&path))) {
        if !side.blocks.is_empty() {
            saida.push_str("\nANOTACOES DOS BLOCOS:\n");
            for bloco in side.blocks.iter().take(20) {
                saida.push_str(&format!(
                    "- {} [{}] what: {} | why: {} | how: {}\n",
                    bloco.id, bloco.kind, bloco.w5h2.what, bloco.w5h2.why, bloco.w5h2.how
                ));
            }
            log_evento(
                "tool_read_annotations",
                serde_json::json!({"file": ws.rel(&path), "blocks": side.blocks.len()}),
            );
        }
    }
    Ok(saida)
}

fn texto_buscar(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let termo = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if termo.is_empty() {
        return Err("query vazia".to_string());
    }
    let mut saida = String::new();
    let mut achados = 0usize;
    fn andar(ws: &Workspace, dir: &Path, termo: &str, saida: &mut String, achados: &mut usize) {
        let Ok(entradas) = fs::read_dir(dir) else {
            return;
        };
        for item in entradas.filter_map(|e| e.ok()) {
            if *achados >= 80 {
                return;
            }
            let nome = item.file_name().to_string_lossy().to_string();
            if matches!(nome.as_str(), "target" | ".git" | "node_modules" | ".c2h") {
                continue;
            }
            let path = item.path();
            if path.is_dir() {
                andar(ws, &path, termo, saida, achados);
                continue;
            }
            let Ok(meta) = item.metadata() else {
                continue;
            };
            if meta.len() > 1_500_000 {
                continue;
            }
            let Ok(texto) = fs::read_to_string(&path) else {
                continue;
            };
            for (i, linha) in texto.lines().enumerate() {
                if linha.to_lowercase().contains(termo) {
                    saida.push_str(&format!("{}:{}: {}\n", ws.rel(&path), i + 1, linha.trim()));
                    *achados += 1;
                    if *achados >= 80 {
                        return;
                    }
                }
            }
        }
    }
    andar(ws, &ws.cwd_abs(), &termo, &mut saida, &mut achados);
    Ok(format!("{achados} resultado(s)\n{saida}"))
}

fn ferramenta_write(
    ws: &Workspace,
    args: &serde_json::Value,
    auto: bool,
    tarefa: &str,
) -> Result<String, String> {
    let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let alvo = ws.resolver(rel, false)?;
    if alvo.exists() {
        return Err(format!("arquivo ja existe: {rel}; use edit_file"));
    }
    let envelope = WriteEnvelope {
        path: rel.to_string(),
        content: content.to_string(),
        imports: vec![],
        summary: summary.to_string(),
        w5h2: W5h2 {
            what: summary.to_string(),
            why: "solicitado pelo agente".to_string(),
            ..W5h2::default()
        },
    };
    validar_novo(ws, &alvo, &envelope, tarefa)?;
    revisar_requisitos(&envelope.content, tarefa)?;
    println!(
        "\n--- arquivo proposto: {rel} ({} linhas) ---",
        content.lines().count()
    );
    for (i, linha) in content.lines().take(120).enumerate() {
        println!("{:>4}: {linha}", i + 1);
    }
    if !auto && !confirmar("criar arquivo?")? {
        return Ok("cancelado pelo usuario".to_string());
    }
    escrever_atomico(&alvo, content)?;
    cmd_annotate(Path::new(rel), false)?;
    if precisa_anotacao(Path::new(rel)) {
        if let Err(erro) = anotar_blocos_ia(Path::new(&ws.rel(&alvo))) {
            println!("aviso: annotation pass falhou: {erro}");
        }
    }
    if let Some(mut side) = ler_sidecar(Path::new(rel)) {
        if let Some(primeiro) = side.blocks.first_mut() {
            primeiro.w5h2 = envelope.w5h2.clone();
        }
        escrever_sidecar(Path::new(rel), &side)?;
    }
    let avisos = marcar_qualidade(Path::new(rel));
    log_evento(
        "tool_write",
        serde_json::json!({"path": rel, "lines": content.lines().count()}),
    );
    let mut resultado = format!("arquivo criado: {rel} ({} linhas)", content.lines().count());
    if !avisos.is_empty() {
        resultado.push_str("\nAVISOS ESTRUTURAIS:\n");
        resultado.push_str(&avisos.join("\n"));
    }
    Ok(resultado)
}

fn ferramenta_edit(ws: &Workspace, args: &serde_json::Value, auto: bool) -> Result<String, String> {
    let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let block_id = args.get("block_id").and_then(|v| v.as_str()).unwrap_or("");
    let new_code = args.get("new_code").and_then(|v| v.as_str()).unwrap_or("");
    let path = ws.resolver(rel, true)?;
    let rel = ws.rel(&path);
    let envelope = ResponseEnvelope {
        contract_version: 1,
        changes: vec![Change {
            file: rel.clone(),
            block_id: block_id.to_string(),
            action: "update".to_string(),
            new_code: new_code.to_string(),
            w5h2: W5h2 {
                what: args
                    .get("what")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                why: args
                    .get("why")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ..W5h2::default()
            },
            evidence: vec![format!("{rel}:1")],
        }],
    };
    let vazio = ler_sidecar(Path::new(&rel))
        .map(|s| s.blocks.is_empty())
        .unwrap_or(true);
    if vazio {
        anotar_blocos_ia(Path::new(&rel))?;
    }
    let violacoes = validar_resposta(Path::new("."), &envelope);
    if !violacoes.is_empty() {
        return Err(serde_json::to_string_pretty(&violacoes).map_err(|e| e.to_string())?);
    }
    if !auto && !confirmar(&format!("editar {rel}?"))? {
        return Ok("cancelado pelo usuario".to_string());
    }
    aplicar_envelope(Path::new("."), &envelope, false)?;
    Ok(format!("editado: {rel}"))
}

fn ferramenta_test() -> Result<String, String> {
    let Ok(comando) = env::var("C2H_TEST_CMD") else {
        return Ok("C2H_TEST_CMD nao configurado".to_string());
    };
    #[cfg(windows)]
    let saida = std::process::Command::new("cmd")
        .args(["/C", &comando])
        .output();
    #[cfg(not(windows))]
    let saida = std::process::Command::new("sh")
        .args(["-c", &comando])
        .output();
    match saida {
        Ok(out) => {
            let texto = format!(
                "exit={}\nstdout:\n{}\nstderr:\n{}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            Ok(texto.chars().take(8000).collect())
        }
        Err(e) => Err(format!("teste: {e}")),
    }
}

fn executar_ferramenta(
    ws: &Workspace,
    auto: bool,
    tarefa: &str,
    nome: &str,
    argumentos: &str,
) -> Result<String, String> {
    let args: serde_json::Value =
        serde_json::from_str(argumentos).unwrap_or_else(|_| serde_json::json!({}));
    log_evento("tool_call", serde_json::json!({"name": nome, "args": args}));
    let resultado = match nome {
        "list_dir" => texto_listar(ws, &args),
        "read_file" => texto_ler(ws, &args),
        "find_text" => texto_buscar(ws, &args),
        "write_file" => ferramenta_write(ws, &args, auto, tarefa),
        "edit_file" => ferramenta_edit(ws, &args, auto),
        "new_component" => {
            let nome = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
            cmd_new_component(ws, nome).map(|_| format!("componente criado: {nome}"))
        }
        "move_file" => {
            let from = args.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
            cmd_mv(ws, &format!("{from} {to}")).map(|_| format!("movido: {from} -> {to}"))
        }
        "remove_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            cmd_rm(ws, path).map(|_| format!("removido: {path}"))
        }
        "run_test" => ferramenta_test(),
        outro => Err(format!("ferramenta desconhecida: {outro}")),
    };
    log_evento(
        "tool_result",
        serde_json::json!({"name": nome, "ok": resultado.is_ok()}),
    );
    resultado
}

fn executar_agente(
    system: &str,
    historico: &mut Vec<serde_json::Value>,
    ws: &Workspace,
    auto: bool,
) -> Result<(), String> {
    let tools = ferramentas();
    let tarefa = historico
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user"))
        .and_then(|m| m.get("content").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    for passo in 0..8 {
        let mut mensagens = vec![serde_json::json!({"role":"system","content":system})];
        mensagens.extend(historico.clone());
        let msg = chamar_api(&mensagens, Some(&tools))?;
        let tool_calls = msg
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if tool_calls.is_empty() {
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            println!("\n{content}\n");
            historico.push(msg);
            return Ok(());
        }
        println!(
            "agente: {} ferramenta(s) no passo {}",
            tool_calls.len(),
            passo + 1
        );
        historico.push(msg);
        for call in tool_calls {
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("call");
            let nome = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let argumentos = call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            println!("  -> {nome}");
            let resultado = executar_ferramenta(ws, auto, &tarefa, nome, argumentos)?;
            let truncado: String = resultado.chars().take(1200).collect();
            historico.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": truncado
            }));
        }
    }
    Err("agente excedeu 8 passos".to_string())
}

fn sidecars_do_projeto() -> Vec<Sidecar> {
    let dir = Path::new(".c2h/blocks");
    let Ok(entradas) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entradas
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<Sidecar>(&t).ok())
        .collect()
}

fn compilar_contexto(ws: &Workspace, tarefa: &str, ativo: Option<&str>) -> String {
    let tokens: Vec<String> = tarefa
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect();
    let cwd_rel = ws.cwd.to_string_lossy().replace('\\', "/");
    let mut sidecars = sidecars_do_projeto();
    sidecars.sort_by_key(|s| {
        std::cmp::Reverse({
            let mut score = 0usize;
            if ativo == Some(s.file.as_str()) {
                score += 1000;
            }
            if !cwd_rel.is_empty() && s.file.starts_with(&cwd_rel) {
                score += 100;
            }
            for bloco in &s.blocks {
                let texto =
                    format!("{} {} {}", bloco.id, bloco.w5h2.what, bloco.w5h2.why).to_lowercase();
                score += tokens.iter().filter(|t| texto.contains(t.as_str())).count() * 20;
            }
            score
        })
    });

    let mut out = String::new();
    out.push_str("<contexto_do_harness>\n");
    out.push_str(&format!("PROJETO: {}\n", ws.root.display()));
    out.push_str(&format!("CWD: /{cwd_rel}\n"));
    if let Some(ativo) = ativo {
        out.push_str(&format!("ARQUIVO ATIVO: {ativo}\n"));
    }

    let mut blocos_total = 0usize;
    let mut arquivos = 0usize;
    for side in sidecars.iter().take(5) {
        if side.blocks.is_empty() {
            continue;
        }
        arquivos += 1;
        out.push_str(&format!(
            "\nARQUIVO: {} (status: {})\n",
            side.file, side.status
        ));
        for bloco in side.blocks.iter().take(12) {
            blocos_total += 1;
            out.push_str(&format!(
                "- [{}] {} linhas {}-{} | what: {} | why: {} | how: {}\n",
                bloco.kind,
                bloco.id,
                bloco.start_line,
                bloco.end_line,
                bloco.w5h2.what,
                bloco.w5h2.why,
                bloco.w5h2.how
            ));
        }
    }

    if let Some(ativo) = ativo {
        if let Ok(texto) = fs::read_to_string(ativo) {
            let ext = Path::new(ativo)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let imports = importacoes(&texto, ext);
            if !imports.is_empty() {
                out.push_str("\nIMPORTS DO ARQUIVO ATIVO:\n");
                for imp in imports.iter().take(20) {
                    out.push_str(&format!("- {imp}\n"));
                }
            }
        }
    }

    if let Ok(saida) = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .output()
    {
        if saida.status.success() {
            let diff = String::from_utf8_lossy(&saida.stdout);
            if !diff.trim().is_empty() {
                out.push_str("\nDIFF ATUAL:\n");
                out.push_str(&diff.chars().take(2000).collect::<String>());
            }
        }
    }
    out.push_str("</contexto_do_harness>");
    let truncado: String = out.chars().take(10000).collect();
    log_evento(
        "context_compiled",
        serde_json::json!({"chars": truncado.len(), "files": arquivos, "blocks": blocos_total, "active": ativo}),
    );
    truncado
}

fn cmd_chat() -> Result<(), String> {
    use std::io::BufRead;
    if !std::io::stdin().is_terminal() && env::var("C2H_FORCE_CHAT").is_err() {
        return Err("modo interativo exige um terminal; use um subcomando".to_string());
    }
    let mut ws = Workspace::novo()?;
    println!("Bom dia! Sou o c2h, seu harness de contexto por blocos.");
    println!("projeto: {}", ws.root.display());
    println!("modelo: {} - provider: {}", model(), base_url());
    if api_key().is_none() {
        println!("aviso: api_key ausente; configure config.json antes de conversar.");
    }
    println!("digite /help para comandos; /sair para encerrar.\n");

    let system_base = "Voce e o c2h, um agente de codigo em terminal. Responda de forma curta, pratica e em portugues. Use as ferramentas para listar, ler, buscar e escrever arquivos. Quando o usuario pedir para criar ou editar algo, chame write_file/new_component/edit_file em vez de apenas mostrar codigo. Nao invente arquivos ou APIs.";
    let mut historico: Vec<serde_json::Value> = Vec::new();
    let mut arquivo: Option<String> = None;
    let mut auto = false;
    let stdin = std::io::stdin();
    loop {
        print!("c2h> ");
        let _ = std::io::stdout().flush();
        let mut linha = String::new();
        if stdin
            .lock()
            .read_line(&mut linha)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        let entrada = linha.trim();
        if entrada.is_empty() {
            continue;
        }
        if entrada == "/sair" || entrada == "/exit" || entrada == "/quit" {
            break;
        }
        if entrada == "/help" || entrada == "/ajuda" {
            println!(
                "\nComandos:\n  /pwd               diretorio atual\n  /ls [dir]          lista arquivos\n  /cd <dir>          muda de diretorio\n  /tree [n]          arvore do projeto\n  /read <f> [a:b]    le arquivo\n  /find <texto>      busca no projeto\n  /imports <f>       lista imports/links\n  /write <f> \"txt\"  cria arquivo com IA\n  /edit <f> \"txt\"   edita por blocos\n  /new component <n> scaffold HTML/CSS/JS\n  /rm <f>            remove com backup\n  /mv <a> <b>        move com backup\n  /preview <f>       abre HTML no navegador\n  /annotate <f>      descobre blocos com IA se vazio\n  /auto               ativa/desativa confirmacao de escrita\n  /tools              lista ferramentas do agente\n  /file <f>          anexa contexto\n  /drop              remove arquivo ativo\n  /clear             limpa historico\n  /status            estado da sessao\n  /sair              encerra\n"
            );
            continue;
        }
        if entrada == "/clear" {
            historico.clear();
            println!("historico limpo");
            continue;
        }
        if entrada == "/drop" {
            arquivo = None;
            println!("arquivo ativo removido");
            continue;
        }
        if entrada == "/pwd" {
            let cwd = if ws.cwd.as_os_str().is_empty() {
                "/".to_string()
            } else {
                format!("/{}", ws.cwd.to_string_lossy().replace('\\', "/"))
            };
            println!("{cwd}");
            continue;
        }
        if entrada == "/status" {
            println!("projeto: {}", ws.root.display());
            println!("cwd: {}", ws.cwd.display());
            println!("modelo: {} - provider: {}", model(), base_url());
            println!("arquivo ativo: {}", arquivo.as_deref().unwrap_or("nenhum"));
            println!("mensagens: {}", historico.len());
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/ls ") {
            if let Err(erro) = listar_dir(&ws, Some(resto.trim())) {
                println!("erro: {erro}");
            }
            continue;
        }
        if entrada == "/ls" {
            if let Err(erro) = listar_dir(&ws, None) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/cd ") {
            match ws.entrar(resto.trim()) {
                Ok(()) => println!("cwd: /{}", ws.cwd.to_string_lossy().replace('\\', "/")),
                Err(erro) => println!("erro: {erro}"),
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/tree") {
            let depth = resto.trim().parse::<usize>().unwrap_or(3);
            if let Err(erro) = arvore(&ws, depth) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/read ") {
            if let Err(erro) = ler_arquivo_chat(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/find ") {
            if let Err(erro) = buscar_texto(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/annotate ") {
            match ws.resolver(resto.trim(), true) {
                Ok(path) => {
                    let rel = ws.rel(&path);
                    if let Err(erro) = cmd_annotate(Path::new(&rel), false) {
                        println!("erro: {erro}");
                        continue;
                    }
                    if precisa_anotacao(Path::new(&rel)) {
                        match anotar_blocos_ia(Path::new(&rel)) {
                            Ok(true) => {}
                            Ok(false) => println!("annotation pass nao encontrou blocos"),
                            Err(erro) => println!("annotation pass falhou: {erro}"),
                        }
                    }
                    marcar_qualidade(Path::new(&rel));
                    println!("anotado: {rel}");
                }
                Err(erro) => println!("erro: {erro}"),
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/imports ") {
            if let Err(erro) = cmd_imports(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/write ") {
            if let Err(erro) = cmd_write(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/new component ") {
            if let Err(erro) = cmd_new_component(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/edit ") {
            if let Err(erro) = cmd_edit(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/rm ") {
            if let Err(erro) = cmd_rm(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/mv ") {
            if let Err(erro) = cmd_mv(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/preview ") {
            if let Err(erro) = cmd_preview(&ws, resto.trim()) {
                println!("erro: {erro}");
            }
            continue;
        }
        if entrada == "/auto" {
            auto = !auto;
            println!("auto={auto} (escrita sem confirmacao)");
            continue;
        }
        if entrada == "/tools" {
            if let Some(lista) = ferramentas().as_array() {
                for tool in lista {
                    if let Some(nome) = tool.pointer("/function/name").and_then(|v| v.as_str()) {
                        println!("- {nome}");
                    }
                }
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/file ") {
            match ws.resolver(resto.trim(), true) {
                Ok(path) => {
                    let rel = ws.rel(&path);
                    if let Err(erro) = montar_contexto(Path::new(&rel)) {
                        println!("erro: {erro}");
                    } else {
                        arquivo = Some(rel.clone());
                        println!("contexto anexado: {rel}");
                    }
                }
                Err(erro) => println!("erro: {erro}"),
            }
            continue;
        }
        let contexto = compilar_contexto(&ws, entrada, arquivo.as_deref());
        let system = format!("{system_base}\n\n{contexto}");
        historico.push(serde_json::json!({"role":"user","content":entrada}));
        if let Err(erro) = executar_agente(&system, &mut historico, &ws, auto) {
            println!("\nerro: {erro}\n");
        }
        while historico.len() > 40 {
            historico.remove(0);
        }
    }
    println!("ate mais.");
    Ok(())
}

fn aplicar_envelope(raiz: &Path, envelope: &ResponseEnvelope, dry_run: bool) -> Result<(), String> {
    log_evento(
        "apply_start",
        serde_json::json!({"changes": envelope.changes.len(), "dry_run": dry_run}),
    );
    let violacoes = validar_resposta(raiz, envelope);
    if !violacoes.is_empty() {
        return Err(serde_json::to_string_pretty(&violacoes).map_err(|e| e.to_string())?);
    }
    let mut por_arquivo: BTreeMap<String, Vec<&Change>> = BTreeMap::new();
    for c in &envelope.changes {
        por_arquivo.entry(c.file.clone()).or_default().push(c);
    }
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (arquivo, changes) in &por_arquivo {
        let path = raiz.join(arquivo);
        let original = fs::read_to_string(&path).unwrap_or_default();
        let mut linhas: Vec<String> = original.lines().map(|l| l.to_string()).collect();
        let (_, blocos) = carregar_arquivo(Path::new(arquivo))?;
        let mut ordenadas: Vec<_> = changes
            .iter()
            .filter_map(|c| {
                if let Some(b) = blocos.iter().find(|b| b.id == c.block_id) {
                    return Some((b.start_line, b.end_line, *c));
                }
                let side = ler_sidecar(Path::new(arquivo));
                side.and_then(|s| {
                    s.blocks
                        .iter()
                        .find(|b| b.id == c.block_id && b.start_line > 0)
                        .map(|b| (b.start_line, b.end_line, *c))
                })
            })
            .collect();
        ordenadas.sort_by_key(|(inicio, _, _)| std::cmp::Reverse(*inicio));
        for (inicio, fim, change) in ordenadas {
            let start = inicio.saturating_sub(1);
            let end = fim.min(linhas.len());
            match change.action.as_str() {
                "delete" => {
                    linhas.drain(start..end);
                }
                "update" => {
                    let novos: Vec<String> =
                        change.new_code.lines().map(|l| l.to_string()).collect();
                    linhas.splice(start..end, novos);
                }
                "create" => {
                    linhas.extend(change.new_code.lines().map(|l| l.to_string()));
                }
                _ => {}
            }
        }
        let novo = linhas.join("\n") + "\n";
        if dry_run {
            let qtd = envelope
                .changes
                .iter()
                .filter(|c| &c.file == arquivo)
                .count();
            println!("dry-run {}: {} mudancas", arquivo, qtd);
        } else {
            let backup = format!("{}.c2h.bak", path.display());
            fs::copy(&path, &backup).map_err(|e| format!("backup: {e}"))?;
            backups.push((path.clone(), PathBuf::from(backup)));
            fs::write(&path, &novo).map_err(|e| format!("gravar {}: {e}", path.display()))?;
            log_evento(
                "apply_write",
                serde_json::json!({"file": arquivo, "chars": novo.len()}),
            );
            if let Err(erro) = rodar_formatter(&path) {
                log_evento(
                    "formatter_fail",
                    serde_json::json!({"file": arquivo, "error": erro}),
                );
                restaurar_backups(&backups);
                return Err(format!("formatter falhou; rollback: {erro}"));
            }
        }
    }
    if !dry_run {
        if let Err(erro) = rodar_testes() {
            log_evento("test_gate_fail", serde_json::json!({"error": erro}));
            restaurar_backups(&backups);
            return Err(erro);
        }
        log_evento(
            "test_gate_ok",
            serde_json::json!({"files": por_arquivo.len()}),
        );
        for (arquivo, changes) in &por_arquivo {
            let anterior = ler_sidecar(Path::new(arquivo));
            cmd_annotate(Path::new(arquivo), false)?;
            let atual = ler_sidecar(Path::new(arquivo));
            let zerou = atual.as_ref().map(|s| s.blocks.is_empty()).unwrap_or(true);
            let tinha_blocos = anterior
                .as_ref()
                .map(|s| !s.blocks.is_empty())
                .unwrap_or(false);
            if zerou && tinha_blocos && anotar_blocos_ia(Path::new(arquivo)).is_err() {
                if let Some(mut antigo) = anterior {
                    antigo.status = "stale".to_string();
                    antigo.source_hash = hash(&fs::read_to_string(arquivo).unwrap_or_default());
                    escrever_sidecar(Path::new(arquivo), &antigo)?;
                    log_evento("sidecar_stale", serde_json::json!({"file": arquivo}));
                }
            }
            atualizar_anotacoes(Path::new(arquivo), changes)?;
        }
        for (_, backup) in &backups {
            let _ = fs::remove_file(backup);
        }
    }
    Ok(())
}

fn restaurar_backups(backups: &[(PathBuf, PathBuf)]) {
    for (original, backup) in backups {
        let _ = fs::copy(backup, original);
        let _ = fs::remove_file(backup);
    }
}

fn rodar_testes() -> Result<(), String> {
    let Ok(comando) = env::var("C2H_TEST_CMD") else {
        return Ok(());
    };
    if comando.trim().is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &comando]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", &comando]);
        c
    };
    let status = cmd.status().map_err(|e| format!("teste: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "testes falharam (exit {status}); rollback aplicado"
        ))
    }
}

fn atualizar_anotacoes(file: &Path, changes: &[&Change]) -> Result<(), String> {
    let Some(mut sidecar) = ler_sidecar(file) else {
        return Ok(());
    };
    for change in changes {
        if let Some(bloco) = sidecar.blocks.iter_mut().find(|b| b.id == change.block_id) {
            if !change.w5h2.what.trim().is_empty() || !change.w5h2.why.trim().is_empty() {
                bloco.w5h2 = change.w5h2.clone();
            }
            if !change.evidence.is_empty() {
                bloco.evidence = change.evidence.clone();
            }
        }
    }
    escrever_sidecar(file, &sidecar)
}

fn rodar_formatter(path: &Path) -> Result<(), String> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => {
            let status = std::process::Command::new("rustfmt")
                .args(["--edition", "2021", path.to_string_lossy().as_ref()])
                .status();
            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("rustfmt exit {s}")),
                Err(e) => Err(format!("rustfmt indisponivel: {e}")),
            }
        }
        _ => Ok(()),
    }
}

fn agora_iso() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let dias = segundos.div_euclid(86_400);
    let resto = segundos.rem_euclid(86_400);
    let z = dias + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut ano = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    if mes <= 2 {
        ano += 1;
    }
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

fn hash(texto: &str) -> String {
    let mut h = Sha256::new();
    h.update(texto.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

fn normalizar_conteudo(linhas: &[&str]) -> String {
    linhas
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn sidecar_path(file: &Path) -> PathBuf {
    let rel = file.to_string_lossy().replace('\\', "/");
    let slug = rel.replace('/', "__");
    PathBuf::from(".c2h")
        .join("blocks")
        .join(format!("{slug}.json"))
}

fn ler_sidecar(file: &Path) -> Option<Sidecar> {
    let texto = fs::read_to_string(sidecar_path(file)).ok()?;
    serde_json::from_str(&texto).ok()
}

fn escrever_sidecar(file: &Path, sidecar: &Sidecar) -> Result<(), String> {
    let path = sidecar_path(file);
    if let Some(pai) = path.parent() {
        fs::create_dir_all(pai).map_err(|e| format!("criar {}: {e}", pai.display()))?;
    }
    let texto = serde_json::to_string_pretty(sidecar).map_err(|e| e.to_string())?;
    fs::write(&path, texto).map_err(|e| format!("gravar {}: {e}", path.display()))
}

fn extrair_blocos(file: &Path, conteudo: &str) -> Vec<Block> {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" => extrair_rust_ast(conteudo).unwrap_or_else(|| extrair_rust(conteudo)),
        "py" => extrair_python_ast(conteudo).unwrap_or_else(|| extrair_python(conteudo)),
        "ts" | "tsx" | "js" | "jsx" => extrair_js(conteudo),
        "html" | "htm" => extrair_html(conteudo),
        "css" => extrair_css(conteudo),
        "md" | "markdown" => extrair_md(conteudo),
        _ => extrair_generico(conteudo),
    }
}

fn bloco_por_linhas(
    file: &Path,
    linhas: &[&str],
    inicio: usize,
    fim: usize,
    kind: &str,
    name: &str,
) -> Block {
    let fim = fim.min(linhas.len());
    let corpo = normalizar_conteudo(&linhas[inicio..fim]);
    let id = format!(
        "{}::{}::{}",
        file.to_string_lossy().replace('\\', "/"),
        kind,
        name
    );
    Block {
        id,
        kind: kind.to_string(),
        name: name.to_string(),
        start_line: inicio + 1,
        end_line: fim,
        semantic_hash: hash(&corpo),
    }
}

fn coletar_ast(node: Node<'_>, conteudo: &str, kinds: &[&str], path: &Path, out: &mut Vec<Block>) {
    if kinds.contains(&node.kind()) {
        if let Some(nome) = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(conteudo.as_bytes()).ok())
        {
            let inicio = node.start_position().row;
            let fim = node.end_position().row + 1;
            let linhas: Vec<&str> = conteudo.lines().collect();
            let kind = match node.kind() {
                "function_item" | "function_definition" => "fn",
                "class_definition" => "class",
                "struct_item" => "struct",
                "impl_item" => "impl",
                "mod_item" => "mod",
                _ => "block",
            };
            out.push(bloco_por_linhas(path, &linhas, inicio, fim, kind, nome));
        }
    }
    let mut cursor = node.walk();
    for filho in node.children(&mut cursor) {
        coletar_ast(filho, conteudo, kinds, path, out);
    }
}

fn extrair_rust_ast(conteudo: &str) -> Option<Vec<Block>> {
    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(conteudo, None)?;
    let mut out = Vec::new();
    coletar_ast(
        tree.root_node(),
        conteudo,
        &["function_item", "struct_item", "impl_item", "mod_item"],
        Path::new("x.rs"),
        &mut out,
    );
    Some(out)
}

fn extrair_python_ast(conteudo: &str) -> Option<Vec<Block>> {
    let mut parser = TsParser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(conteudo, None)?;
    let mut out = Vec::new();
    coletar_ast(
        tree.root_node(),
        conteudo,
        &["function_definition", "class_definition"],
        Path::new("x.py"),
        &mut out,
    );
    Some(out)
}

fn extrair_rust(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        let t = linha.trim();
        if let Some(pos) = t.find("fn ") {
            if pos > 0
                && !t[..pos].trim().is_empty()
                && !t.starts_with("pub ")
                && !t.starts_with("async ")
                && !t.starts_with("unsafe ")
                && !t.starts_with("const ")
            {
                // metodos dentro de impl tambem entram; apenas evita matches estranhos
            }
            let resto = &t[pos + 3..];
            let nome = resto
                .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("anon");
            if nome.is_empty() || nome == "main" && false {
                continue;
            }
            let mut prof = 0i32;
            let mut fim = i;
            for (j, l) in linhas.iter().enumerate().skip(i) {
                prof += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                fim = j + 1;
                if prof <= 0 && j > i {
                    break;
                }
            }
            out.push(bloco_por_linhas(
                Path::new("x.rs"),
                &linhas,
                i,
                fim,
                "fn",
                nome,
            ));
        }
    }
    out
}

fn extrair_python(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        let t = linha.trim_start();
        if let Some(resto) = t
            .strip_prefix("def ")
            .or_else(|| t.strip_prefix("async def "))
        {
            let nome = resto.split(['(', ':']).next().unwrap_or("anon").trim();
            let indent = linha.len() - t.len();
            let mut fim = i + 1;
            for (j, l) in linhas.iter().enumerate().skip(i + 1) {
                let lt = l.trim_start();
                if !lt.is_empty() && (l.len() - lt.len()) <= indent {
                    break;
                }
                fim = j + 1;
            }
            out.push(bloco_por_linhas(
                Path::new("x.py"),
                &linhas,
                i,
                fim,
                "def",
                nome,
            ));
        } else if let Some(resto) = t.strip_prefix("class ") {
            let nome = resto.split(['(', ':']).next().unwrap_or("anon").trim();
            out.push(bloco_por_linhas(
                Path::new("x.py"),
                &linhas,
                i,
                linhas.len(),
                "class",
                nome,
            ));
        }
    }
    out
}

fn extrair_js(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        let t = linha.trim();
        let candidato = t
            .strip_prefix("function ")
            .or_else(|| t.strip_prefix("export function "))
            .or_else(|| t.strip_prefix("class "))
            .or_else(|| t.strip_prefix("export class "));
        if let Some(resto) = candidato {
            let nome = resto.split(['(', '{', '<']).next().unwrap_or("anon").trim();
            let mut prof = 0i32;
            let mut fim = i + 1;
            for (j, l) in linhas.iter().enumerate().skip(i) {
                prof += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                fim = j + 1;
                if prof <= 0 && j > i {
                    break;
                }
            }
            out.push(bloco_por_linhas(
                Path::new("x.ts"),
                &linhas,
                i,
                fim,
                "fn",
                nome,
            ));
        }
    }
    out
}

fn extrair_html(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        let t = linha.trim();
        if !t.starts_with('<') || !(t.starts_with("<section") || t.contains("data-c2-block")) {
            continue;
        }
        let tag = t
            .trim_start_matches('<')
            .split(|c: char| c.is_whitespace() || c == '>')
            .next()
            .unwrap_or("div")
            .to_string();
        let nome = t
            .split("data-c2-block=\"")
            .nth(1)
            .and_then(|x| x.split('"').next())
            .or_else(|| t.split("id=\"").nth(1).and_then(|x| x.split('"').next()))
            .unwrap_or("anon")
            .to_string();
        let abre = format!("<{tag}");
        let fecha = format!("</{tag}>");
        let mut prof = 0i32;
        let mut fim = i + 1;
        for (j, l) in linhas.iter().enumerate().skip(i) {
            prof += l.matches(&abre).count() as i32;
            prof -= l.matches(&fecha).count() as i32;
            fim = j + 1;
            if prof <= 0 && j > i {
                break;
            }
        }
        out.push(bloco_por_linhas(
            Path::new("x.html"),
            &linhas,
            i,
            fim,
            "section",
            &nome,
        ));
    }
    out
}

fn extrair_css(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < linhas.len() {
        let t = linhas[i].trim();
        if t.contains('{') && !t.starts_with('@') {
            let seletor = t.split('{').next().unwrap_or("rule").trim();
            let mut prof = 0i32;
            let mut fim = i + 1;
            for (j, l) in linhas.iter().enumerate().skip(i) {
                prof += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                fim = j + 1;
                if prof <= 0 && j > i {
                    break;
                }
            }
            out.push(bloco_por_linhas(
                Path::new("x.css"),
                &linhas,
                i,
                fim,
                "css-rule",
                seletor,
            ));
            i = fim;
        } else {
            i += 1;
        }
    }
    out
}

fn extrair_md(conteudo: &str) -> Vec<Block> {
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut out = Vec::new();
    for (i, linha) in linhas.iter().enumerate() {
        let t = linha.trim();
        if let Some(nivel) = t.chars().take_while(|c| *c == '#').count().checked_sub(1) {
            let nome = t.trim_start_matches('#').trim();
            let mut fim = linhas.len();
            for (j, l) in linhas.iter().enumerate().skip(i + 1) {
                let lt = l.trim();
                let n = lt.chars().take_while(|c| *c == '#').count();
                if n > 0 && n <= nivel + 1 {
                    fim = j;
                    break;
                }
            }
            out.push(bloco_por_linhas(
                Path::new("x.md"),
                &linhas,
                i,
                fim,
                "heading",
                nome,
            ));
        }
    }
    out
}

fn extrair_generico(_conteudo: &str) -> Vec<Block> {
    Vec::new()
}

fn rel_file(file: &Path) -> String {
    file.to_string_lossy().replace('\\', "/")
}

fn carregar_arquivo(file: &Path) -> Result<(String, Vec<Block>), String> {
    let conteudo = fs::read_to_string(file).map_err(|e| format!("ler {}: {e}", file.display()))?;
    let mut blocos = extrair_blocos(file, &conteudo);
    // normaliza o path do id e o arquivo no bloco
    let rel = rel_file(file);
    for bloco in &mut blocos {
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.rs").to_string_lossy()),
            &format!("{rel}::"),
        );
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.py").to_string_lossy()),
            &format!("{rel}::"),
        );
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.ts").to_string_lossy()),
            &format!("{rel}::"),
        );
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.html").to_string_lossy()),
            &format!("{rel}::"),
        );
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.css").to_string_lossy()),
            &format!("{rel}::"),
        );
        bloco.id = bloco.id.replace(
            &format!("{}::", Path::new("x.md").to_string_lossy()),
            &format!("{rel}::"),
        );
    }
    Ok((conteudo, blocos))
}

static DEBUG_ATIVO: AtomicBool = AtomicBool::new(false);
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn iniciar_debug(flag: bool) {
    let env_flag = env::var("C2H_DEBUG")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    if !(flag || env_flag) {
        return;
    }
    DEBUG_ATIVO.store(true, Ordering::Relaxed);
    let dir = Path::new(".c2h/logs");
    if fs::create_dir_all(dir).is_ok() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = dir.join(format!("run-{stamp}-{}.jsonl", std::process::id()));
        let _ = LOG_PATH.set(path);
    }
}

fn log_evento(evento: &str, dados: serde_json::Value) {
    if !DEBUG_ATIVO.load(Ordering::Relaxed) {
        return;
    }
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    let linha = serde_json::json!({
        "ts": agora_iso(),
        "event": evento,
        "data": dados,
    });
    if let Ok(mut arquivo) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(arquivo, "{linha}");
    }
}

fn cmd_logs(limit: usize) -> Result<(), String> {
    let dir = Path::new(".c2h/logs");
    if !dir.is_dir() {
        println!("sem logs em .c2h/logs");
        return Ok(());
    }
    let mut arquivos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"))
        .collect();
    arquivos.sort();
    let Some(ultimo) = arquivos.last() else {
        println!("sem logs");
        return Ok(());
    };
    let texto = fs::read_to_string(ultimo).map_err(|e| e.to_string())?;
    let linhas: Vec<&str> = texto.lines().collect();
    let inicio = linhas.len().saturating_sub(limit);
    for linha in &linhas[inicio..] {
        println!("{linha}");
    }
    Ok(())
}

fn cmd_fuzz(iters: usize) -> Result<(), String> {
    let mut estado = 0x9E3779B97F4A7C15u64;
    let mut ok = 0usize;
    let mut rejeitados = 0usize;
    for i in 0..iters {
        estado ^= estado << 13;
        estado ^= estado >> 7;
        estado ^= estado << 17;
        let escolha = estado % 8;
        let payload = match escolha {
            0 => "{}".to_string(),
            1 => "{\"contract_version\":1}".to_string(),
            2 => "{\"contract_version\":1,\"changes\":[]}".to_string(),
            3 => "{\"contract_version\":1,\"changes\":[{\"file\":\"../x\",\"block_id\":\"x\",\"action\":\"update\",\"new_code\":\"x\"}]}".to_string(),
            4 => format!("{{\"contract_version\":1,\"changes\":[{{\"file\":\"a.rs\",\"block_id\":\"x{},", estado),
            5 => "```json\n{\"contract_version\":1,\"changes\":[]}\n```".to_string(),
            6 => format!("{{\"contract_version\":{},\"changes\":[]}}", estado % 4),
            _ => "not json at all".to_string(),
        };
        match extrair_json(&payload) {
            Ok(envelope) => {
                ok += 1;
                let violacoes = validar_resposta(Path::new("."), &envelope);
                if violacoes.is_empty() {
                    log_evento("fuzz_ok", serde_json::json!({"i": i}));
                }
            }
            Err(_) => rejeitados += 1,
        }
    }
    log_evento(
        "fuzz_end",
        serde_json::json!({"iters": iters, "ok": ok, "rejeitados": rejeitados}),
    );
    println!("iters={iters} ok={ok} rejeitados={rejeitados}");
    Ok(())
}

fn nome_comando(command: Option<&Command>) -> &'static str {
    match command {
        None | Some(Command::Chat) => "chat",
        Some(Command::Doctor) => "doctor",
        Some(Command::Blocks { .. }) => "blocks",
        Some(Command::Annotate { .. }) => "annotate",
        Some(Command::Validate { .. }) => "validate",
        Some(Command::Status) => "status",
        Some(Command::Context { .. }) => "context",
        Some(Command::Ask { .. }) => "ask",
        Some(Command::ValidateResponse { .. }) => "validate_response",
        Some(Command::Apply { .. }) => "apply",
        Some(Command::Logs { .. }) => "logs",
        Some(Command::Fuzz { .. }) => "fuzz",
        Some(Command::Config) => "config",
    }
}

fn cmd_doctor() -> Result<(), String> {
    println!("c2h {}", env!("CARGO_PKG_VERSION"));
    println!(
        "diretorio: {}",
        env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    );
    println!(".c2h: {}", Path::new(".c2h").exists());
    println!("base_url: {}", base_url());
    println!("model: {}", model());
    println!(
        "api_key: {}",
        if api_key().is_some() {
            "configurada"
        } else {
            "ausente"
        }
    );
    Ok(())
}

fn cmd_blocks(file: &Path, json: bool) -> Result<(), String> {
    let (_, blocos) = carregar_arquivo(file)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&blocos).map_err(|e| e.to_string())?
        );
    } else {
        for b in &blocos {
            println!("{}:{}  {}  {}", b.kind, b.name, b.start_line, b.end_line);
        }
    }
    Ok(())
}

fn violacoes_politica(conteudo: &str, instrucao: &str) -> Vec<String> {
    let mut erros = Vec::new();
    let baixo_instr = instrucao.to_lowercase();
    let baixo = conteudo.to_lowercase();
    let sem_js = baixo_instr.contains("sem javascript")
        || baixo_instr.contains("sem js")
        || baixo_instr.contains("apenas html e css")
        || baixo_instr.contains("so html e css")
        || baixo_instr.contains("só html e css");
    if sem_js {
        if baixo.contains("<script") {
            erros.push("conteudo tem <script> apesar do pedido sem JavaScript".to_string());
        }
        if baixo.contains("javascript:") {
            erros.push("conteudo tem javascript: apesar do pedido sem JavaScript".to_string());
        }
        for evento in [
            "onload=",
            "onclick=",
            "onchange=",
            "onsubmit=",
            "oninput=",
            "addEventListener",
        ] {
            if baixo.contains(evento) {
                erros.push(format!(
                    "conteudo tem {evento} apesar do pedido sem JavaScript"
                ));
            }
        }
    }
    erros
}

fn revisar_requisitos(conteudo: &str, instrucao: &str) -> Result<(), String> {
    if instrucao.trim().is_empty()
        || env::var("C2H_SKIP_REVIEW").is_ok()
        || env::var("C2H_MOCK_RESPONSE").is_ok()
    {
        return Ok(());
    }
    let system = "Voce e um revisor rigoroso. Compare o conteudo com o pedido do usuario. Responda APENAS JSON: {\"compliant\":true|false,\"violations\":[\"...\"],\"fix\":\"...\"}. Nao invente problemas; verifique especialmente pedidos como 'sem JavaScript'.";
    let user = format!(
        "PEDIDO:\n{instrucao}\n\nCONTEUDO:\n{}",
        conteudo.chars().take(12000).collect::<String>()
    );
    let resposta = chamar_provider(system, &user)?;
    let json = extrair_json_valor(&resposta)?;
    let compliant = json
        .get("compliant")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !compliant {
        let violacoes = json
            .get("violations")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| "conteudo nao atende ao pedido".to_string());
        return Err(format!("revisor: {violacoes}"));
    }
    Ok(())
}

fn precisa_anotacao(file: &Path) -> bool {
    let linhas = fs::read_to_string(file)
        .map(|t| t.lines().count())
        .unwrap_or(0);
    let lado = ler_sidecar(file);
    let blocos = lado.map(|s| s.blocks).unwrap_or_default();
    if blocos.is_empty() {
        return true;
    }
    if linhas > 150 && blocos.len() < 2 {
        return true;
    }
    blocos
        .iter()
        .any(|b| b.end_line.saturating_sub(b.start_line) > 150)
}

fn avaliar_granularidade(file: &Path) -> Vec<String> {
    let linhas = fs::read_to_string(file)
        .map(|t| t.lines().count())
        .unwrap_or(0);
    let blocos = ler_sidecar(file).map(|s| s.blocks).unwrap_or_default();
    let mut avisos = Vec::new();
    if linhas > 150 {
        avisos.push(format!(
            "arquivo tem {linhas} linhas; prefira <= 150 e divida em componentes/secoes"
        ));
    }
    if blocos.len() < 2 && linhas > 120 {
        avisos.push(format!(
            "apenas {} bloco(s) para {linhas} linhas; granularidade baixa",
            blocos.len()
        ));
    }
    if let Some(maior) = blocos
        .iter()
        .map(|b| b.end_line.saturating_sub(b.start_line))
        .max()
    {
        if maior > 150 {
            avisos.push(format!(
                "maior bloco cobre {maior} linhas; divida a responsabilidade"
            ));
        }
    }
    avisos
}

fn sugerir_divisao(conteudo: &str, tarefa: &str) -> Result<(), String> {
    if env::var("C2H_SKIP_REVIEW").is_ok() || env::var("C2H_MOCK_RESPONSE").is_ok() {
        return Ok(());
    }
    let system = "Um arquivo ficou grande. Sugira uma divisao em componentes/secoes pequenos. Responda APENAS JSON: {\"components\":[{\"path\":\"...\",\"responsibility\":\"...\"}]}. Nao escreva codigo, apenas o plano.";
    let user = format!(
        "PEDIDO:\n{tarefa}\n\nARQUIVO ({} linhas):\n{}",
        conteudo.lines().count(),
        conteudo.chars().take(8000).collect::<String>()
    );
    let resposta = chamar_provider(system, &user)?;
    let json = extrair_json_valor(&resposta)?;
    if let Some(lista) = json.get("components").and_then(|v| v.as_array()) {
        println!("reparo sugerido: dividir em {} componentes", lista.len());
        for item in lista {
            let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let resp = item
                .get("responsibility")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("  - {path}: {resp}");
        }
    }
    Ok(())
}

fn marcar_qualidade(file: &Path) -> Vec<String> {
    let avisos = avaliar_granularidade(file);
    if avisos.is_empty() {
        return avisos;
    }
    if let Some(mut side) = ler_sidecar(file) {
        side.status = "weak".to_string();
        let _ = escrever_sidecar(file, &side);
    }
    for aviso in &avisos {
        println!("aviso estrutural: {aviso}");
    }
    log_evento(
        "granularity_warning",
        serde_json::json!({"file": rel_file(file), "warnings": avisos}),
    );
    avisos
}

fn anotar_blocos_ia(file: &Path) -> Result<bool, String> {
    let conteudo = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let linhas: Vec<&str> = conteudo.lines().collect();
    let system = "Voce identifica blocos de responsabilidade em um arquivo. Responda APENAS JSON: {\"blocks\":[{\"block_id\":\"id-unico\",\"kind\":\"section|function|class|block\",\"start_line\":1,\"end_line\":10,\"what\":\"...\",\"why\":\"...\",\"how\":\"...\",\"confidence\":\"high|medium|low\"}]}. Use 1 a 12 blocos, sem sobreposicao. Nao escreva timestamp.";
    let mut user = format!(
        "ARQUIVO: {}\nTotal de linhas: {}\nConteudo:\n{}",
        rel_file(file),
        linhas.len(),
        conteudo.chars().take(16000).collect::<String>()
    );
    let mut ultimo_erro = String::new();
    for tentativa in 0..3 {
        let resposta = chamar_provider(system, &user)?;
        match extrair_json_valor(&resposta) {
            Ok(json) => {
                let Some(lista) = json.get("blocks").and_then(|v| v.as_array()) else {
                    ultimo_erro = "resposta sem blocks[]".to_string();
                    user = format!(
                        "Resposta anterior invalida: {ultimo_erro}. Devolva blocks[].\n{user}"
                    );
                    continue;
                };
                if lista.is_empty() {
                    ultimo_erro = "blocks[] vazio".to_string();
                    user = format!("Resposta anterior invalida: {ultimo_erro}. Identifique pelo menos um bloco.\n{user}");
                    continue;
                }
                let mut blocos = Vec::new();
                let mut ids = std::collections::BTreeSet::new();
                let mut intervalos: Vec<(usize, usize)> = Vec::new();
                let mut erro = None;
                for item in lista.iter().take(12) {
                    let id = item
                        .get("block_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let kind = item
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("block")
                        .to_string();
                    let start =
                        item.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let end = item.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    if id.is_empty()
                        || !ids.insert(id.clone())
                        || start == 0
                        || end < start
                        || end > linhas.len()
                    {
                        erro = Some(format!("bloco invalido: {id} {start}-{end}"));
                        break;
                    }
                    if intervalos.iter().any(|(a, b)| start <= *b && end >= *a) {
                        erro = Some(format!("blocos sobrepostos em {id}"));
                        break;
                    }
                    intervalos.push((start, end));
                    let corpo = normalizar_conteudo(&linhas[start - 1..end]);
                    blocos.push(SidecarBlock {
                        id: id.clone(),
                        kind,
                        name: id.split("::").last().unwrap_or(&id).to_string(),
                        start_line: start,
                        end_line: end,
                        created_at: agora_iso(),
                        updated_at: agora_iso(),
                        semantic_hash: hash(&corpo),
                        w5h2: W5h2 {
                            what: item
                                .get("what")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            why: item
                                .get("why")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            how: item
                                .get("how")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            ..W5h2::default()
                        },
                        evidence: vec![format!("{}:{}", rel_file(file), start)],
                    });
                }
                if let Some(erro_msg) = erro {
                    ultimo_erro = erro_msg.clone();
                    if tentativa < 2 {
                        user = format!("Resposta anterior rejeitada: {erro_msg}. Corrija os blocos e devolva o JSON completo.\n{user}");
                    }
                    continue;
                }
                let sidecar = Sidecar {
                    schema_version: SCHEMA_VERSION,
                    contract_version: CONTRACT_VERSION,
                    file: rel_file(file),
                    status: "annotated".to_string(),
                    source_hash: hash(&conteudo),
                    generated_by: "c2h-ai".to_string(),
                    blocks: blocos,
                };
                escrever_sidecar(file, &sidecar)?;
                log_evento(
                    "annotate_blocks_ai",
                    serde_json::json!({"file": rel_file(file), "blocks": sidecar.blocks.len()}),
                );
                println!(
                    "annotation pass: {} blocos descobertos",
                    sidecar.blocks.len()
                );
                return Ok(true);
            }
            Err(erro) => {
                ultimo_erro = erro.clone();
                if tentativa < 2 {
                    user = format!(
                        "Erro de formato: {erro}. Devolva APENAS JSON com blocks[].\n{user}"
                    );
                }
            }
        }
    }
    Err(format!("annotation pass nao convergiu: {ultimo_erro}"))
}

fn cmd_annotate(file: &Path, dry_run: bool) -> Result<(), String> {
    let (_, blocos) = carregar_arquivo(file)?;
    let anterior = ler_sidecar(file);
    let agora = agora_iso();
    let mut antigos = BTreeMap::new();
    if let Some(sidecar) = &anterior {
        for b in &sidecar.blocks {
            antigos.insert(b.id.clone(), b.clone());
        }
    }
    let mut novos = Vec::new();
    for bloco in blocos {
        let side = if let Some(old) = antigos.get(&bloco.id) {
            let changed = old.semantic_hash != bloco.semantic_hash;
            SidecarBlock {
                id: bloco.id.clone(),
                kind: bloco.kind.clone(),
                name: bloco.name.clone(),
                start_line: bloco.start_line,
                end_line: bloco.end_line,
                created_at: old.created_at.clone(),
                updated_at: if changed {
                    agora.clone()
                } else {
                    old.updated_at.clone()
                },
                semantic_hash: bloco.semantic_hash.clone(),
                w5h2: old.w5h2.clone(),
                evidence: old.evidence.clone(),
            }
        } else {
            SidecarBlock {
                id: bloco.id.clone(),
                kind: bloco.kind.clone(),
                name: bloco.name.clone(),
                start_line: bloco.start_line,
                end_line: bloco.end_line,
                created_at: agora.clone(),
                updated_at: agora.clone(),
                semantic_hash: bloco.semantic_hash.clone(),
                w5h2: W5h2::default(),
                evidence: Vec::new(),
            }
        };
        novos.push(side);
    }
    let source_hash = hash(&fs::read_to_string(file).unwrap_or_default());
    let sidecar = Sidecar {
        schema_version: SCHEMA_VERSION,
        contract_version: CONTRACT_VERSION,
        file: rel_file(file),
        status: if novos.is_empty() {
            "unannotated"
        } else {
            "annotated"
        }
        .to_string(),
        source_hash,
        generated_by: "c2h".to_string(),
        blocks: novos,
    };
    if dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&sidecar).map_err(|e| e.to_string())?
        );
    } else {
        escrever_sidecar(file, &sidecar)?;
        log_evento(
            "annotate",
            serde_json::json!({"file": rel_file(file), "blocks": sidecar.blocks.len()}),
        );
        println!("sidecar: {}", sidecar_path(file).display());
        println!("blocos: {}", sidecar.blocks.len());
    }
    Ok(())
}

fn cmd_status() -> Result<(), String> {
    let dir = Path::new(".c2h/blocks");
    if !dir.is_dir() {
        println!("sem sidecars em .c2h/blocks");
        return Ok(());
    }
    let mut total = 0usize;
    let mut arquivos = 0usize;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(texto) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(sidecar) = serde_json::from_str::<Sidecar>(&texto) else {
            continue;
        };
        arquivos += 1;
        total += sidecar.blocks.len();
    }
    println!("arquivos: {arquivos}");
    println!("blocos: {total}");
    Ok(())
}

fn extrair_json_valor(texto: &str) -> Result<serde_json::Value, String> {
    let limpo = texto
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let inicio = limpo
        .find('{')
        .ok_or_else(|| "resposta sem JSON".to_string())?;
    let fim = limpo
        .rfind('}')
        .ok_or_else(|| "JSON incompleto".to_string())?;
    serde_json::from_str(&limpo[inicio..=fim]).map_err(|e| format!("JSON invalido: {e}"))
}

fn cmd_annotate_ai(file: &Path, dry_run: bool) -> Result<(), String> {
    let (conteudo, blocos) = carregar_arquivo(file)?;
    let linhas: Vec<&str> = conteudo.lines().collect();
    let mut pedido = String::from("Preencha W5H2 dos blocos abaixo. Responda APENAS JSON:\n{\"blocks\":[{\"block_id\":\"...\",\"what\":\"...\",\"why\":\"...\",\"when\":\"...\",\"where\":\"...\",\"who\":\"...\",\"how\":\"...\",\"how_much\":\"...\"}]}\nUse \"unknown\" quando nao souber e nao escreva timestamp.\n\n");
    for bloco in &blocos {
        let inicio = bloco.start_line.saturating_sub(1).min(linhas.len());
        let fim = bloco.end_line.min(linhas.len());
        pedido.push_str(&format!(
            "\nBLOCO: {}\n```\n{}\n```\n",
            bloco.id,
            linhas[inicio..fim].join("\n")
        ));
    }
    eprintln!("enviando {} blocos para a IA...", blocos.len());
    let resposta = chamar_provider(
        "Voce preenche anotacoes 5W2H de blocos de codigo. Nao invente; use unknown. Responda somente JSON.",
        &pedido,
    )?;
    let json = extrair_json_valor(&resposta)?;
    let lista = json
        .get("blocks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "resposta sem blocks[]".to_string())?;
    let Some(mut sidecar) = ler_sidecar(file) else {
        return Err("sidecar ausente; rode c2h annotate primeiro".to_string());
    };
    let mut atualizados = 0usize;
    for item in lista {
        let Some(id) = item.get("block_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(bloco) = sidecar.blocks.iter_mut().find(|b| b.id == id) else {
            continue;
        };
        let texto = |chave: &str| {
            item.get(chave)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        bloco.w5h2 = W5h2 {
            what: texto("what"),
            why: texto("why"),
            when: texto("when"),
            where_: texto("where"),
            who: texto("who"),
            how: texto("how"),
            how_much: texto("how_much"),
        };
        atualizados += 1;
    }
    if dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&sidecar).map_err(|e| e.to_string())?
        );
    } else {
        escrever_sidecar(file, &sidecar)?;
        log_evento(
            "annotate_ai",
            serde_json::json!({"file": rel_file(file), "blocks": atualizados}),
        );
        println!("sidecar: {}", sidecar_path(file).display());
        println!("blocos atualizados: {atualizados}");
    }
    Ok(())
}

fn cmd_validate(file: Option<&Path>) -> Result<(), String> {
    let arquivos: Vec<PathBuf> = match file {
        Some(path) => vec![path.to_path_buf()],
        None => {
            let mut v = Vec::new();
            if Path::new(".c2h/blocks").is_dir() {
                for entry in fs::read_dir(".c2h/blocks").map_err(|e| e.to_string())? {
                    let p = entry.map_err(|e| e.to_string())?.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("json") {
                        v.push(p);
                    }
                }
            }
            v
        }
    };
    let mut ok = true;
    for sidecar_file in arquivos {
        let texto = fs::read_to_string(&sidecar_file).map_err(|e| e.to_string())?;
        let sidecar: Sidecar = serde_json::from_str(&texto).map_err(|e| e.to_string())?;
        let code_file = PathBuf::from(&sidecar.file);
        if !code_file.exists() {
            println!("ERRO {}: arquivo ausente", sidecar.file);
            ok = false;
            continue;
        }
        let (_, atuais) = carregar_arquivo(&code_file)?;
        let atuais: BTreeMap<String, Block> =
            atuais.into_iter().map(|b| (b.id.clone(), b)).collect();
        for bloco in &sidecar.blocks {
            match atuais.get(&bloco.id) {
                None => {
                    println!("ERRO {}: bloco removido/renomeado", bloco.id);
                    ok = false;
                }
                Some(atual) if atual.semantic_hash != bloco.semantic_hash => {
                    println!("WARN {}: anotacao desatualizada", bloco.id);
                }
                _ => {}
            }
        }
    }
    if ok {
        println!("ok");
        Ok(())
    } else {
        Err("validacao encontrou erros".to_string())
    }
}

fn cmd_context(file: &Path, max_chars: usize) -> Result<(), String> {
    let (_, blocos) = carregar_arquivo(file)?;
    let sidecar = ler_sidecar(file);
    let mut texto = String::new();
    texto.push_str(&format!("ARQUIVO: {}\n", rel_file(file)));
    if let Some(s) = sidecar {
        for bloco in s.blocks {
            texto.push_str(&format!(
                "\nBLOCO: {}\nkind: {}\nupdated_at: {}\nwhat: {}\nwhy: {}\nhow: {}\n",
                bloco.id,
                bloco.kind,
                bloco.updated_at,
                bloco.w5h2.what,
                bloco.w5h2.why,
                bloco.w5h2.how
            ));
            if texto.len() > max_chars {
                texto.truncate(max_chars);
                texto.push_str("\n[contexto truncado]");
                break;
            }
        }
    } else {
        for b in blocos {
            texto.push_str(&format!("{}\n", b.id));
        }
    }
    print!("{texto}");
    Ok(())
}

fn config_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("C2H_CONFIG") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(pai) = exe.parent() {
            let path = pai.join("config.json");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let local = PathBuf::from("config.json");
    if local.is_file() {
        return Some(local);
    }
    None
}

fn config_json() -> Option<serde_json::Value> {
    let path = config_path()?;
    let texto = fs::read_to_string(path).ok()?;
    serde_json::from_str(&texto).ok()
}

fn config_str(chave: &str) -> Option<String> {
    config_json()?
        .get(chave)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
        .filter(|v| !v.trim().is_empty())
}

fn api_key() -> Option<String> {
    env::var("C2H_API_KEY")
        .ok()
        .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
        .or_else(|| env::var("OPENCODEGO_API_KEY").ok())
        .or_else(|| config_str("api_key"))
        .filter(|v| !v.trim().is_empty())
}

fn base_url() -> String {
    env::var("C2H_BASE_URL")
        .ok()
        .or_else(|| config_str("base_url"))
        .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string())
}

fn session_id() -> String {
    env::var("C2H_SESSION")
        .ok()
        .or_else(|| config_str("session"))
        .unwrap_or_else(|| {
            format!(
                "c2h-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            )
        })
}

fn model() -> String {
    env::var("C2H_MODEL")
        .ok()
        .or_else(|| config_str("model"))
        .unwrap_or_else(|| "deepseek-chat".to_string())
}

fn cmd_ask(
    prompt: &str,
    system: Option<&str>,
    file: Option<&Path>,
    apply: bool,
    max_repairs: usize,
) -> Result<(), String> {
    if let Some(file) = file {
        if apply {
            return cmd_ask_contract(prompt, file, max_repairs);
        }
        let contexto = montar_contexto(file)?;
        let system_eff: &str = match system {
            Some(valor) => valor,
            None => contract_system(),
        };
        let user = format!("{prompt}\n\nCONTEXTO:\n{contexto}");
        println!("{}", chamar_provider(system_eff, &user)?);
        return Ok(());
    }
    let system = system
        .unwrap_or("Voce e um agente de codigo. Responda de forma curta e nao invente arquivos.");
    println!("{}", chamar_provider(system, prompt)?);
    Ok(())
}

fn cmd_ask_contract(prompt: &str, file: &Path, max_repairs: usize) -> Result<(), String> {
    let contexto = montar_contexto(file)?;
    let mut system = contract_system().to_string();
    let mut user = format!("{prompt}\n\nCONTEXTO:\n{contexto}");
    let mut ultimo_erro = String::new();
    for tentativa in 0..=max_repairs {
        log_evento(
            "contract_attempt",
            serde_json::json!({"attempt": tentativa}),
        );
        let resposta = chamar_provider(&system, &user)?;
        match extrair_json(&resposta) {
            Ok(envelope) => {
                let violacoes = validar_resposta(Path::new("."), &envelope);
                if violacoes.is_empty() {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())?
                    );
                    return aplicar_envelope(Path::new("."), &envelope, false);
                }
                ultimo_erro =
                    serde_json::to_string_pretty(&violacoes).map_err(|e| e.to_string())?;
                if tentativa == max_repairs {
                    break;
                }
                system = format!(
                    "{}\n\nA resposta anterior foi rejeitada. Corrija apenas as violacoes e devolva o envelope completo.",
                    contract_system()
                );
                user = format!(
                    "TAREFA ORIGINAL:\n{prompt}\n\nVIOLATIONS:\n{ultimo_erro}\n\nRESPOSTA ANTERIOR:\n{resposta}"
                );
            }
            Err(erro) => {
                log_evento(
                    "contract_parse_error",
                    serde_json::json!({"attempt": tentativa, "error": erro}),
                );
                ultimo_erro = erro.clone();
                if tentativa == max_repairs {
                    break;
                }
                user = format!(
                    "TAREFA ORIGINAL:\n{prompt}\n\nERRO DE FORMATO: {erro}\n\nDevolva APENAS JSON valido no contrato."
                );
            }
        }
    }
    Err(format!("contrato nao convergiu: {ultimo_erro}"))
}

fn cmd_validate_response(response: &Path) -> Result<(), String> {
    let texto = fs::read_to_string(response).map_err(|e| format!("ler resposta: {e}"))?;
    let envelope = extrair_json(&texto)?;
    let violacoes = validar_resposta(Path::new("."), &envelope);
    if violacoes.is_empty() {
        println!("ok");
        Ok(())
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&violacoes).map_err(|e| e.to_string())?
        );
        Err("contrato invalido".to_string())
    }
}

fn cmd_apply(response: &Path, dry_run: bool) -> Result<(), String> {
    let texto = fs::read_to_string(response).map_err(|e| format!("ler resposta: {e}"))?;
    let envelope = extrair_json(&texto)?;
    aplicar_envelope(Path::new("."), &envelope, dry_run)
}

fn cmd_config() {
    println!("base_url={}", base_url());
    println!("model={}", model());
    println!(
        "api_key={}",
        if api_key().is_some() {
            "configurada"
        } else {
            "ausente"
        }
    );
    println!("contract_version={CONTRACT_VERSION}");
    println!(
        "config_file={}",
        config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "nao encontrado".to_string())
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn extrai_funcoes_rust() {
        let blocos = extrair_blocos(Path::new("x.rs"), "fn a() {\n  let x = 1;\n}\nfn b() {}\n");
        assert_eq!(blocos.len(), 2);
        assert!(blocos.iter().any(|b| b.name == "a"));
    }

    #[test]
    fn extrai_defs_python() {
        let blocos = extrair_blocos(
            Path::new("x.py"),
            "def a():\n    return 1\n\ndef b():\n    pass\n",
        );
        assert_eq!(blocos.len(), 2);
    }

    #[test]
    #[test]
    fn politica_bloqueia_script_quando_sem_js() {
        let erros = violacoes_politica("<html><script>alert(1)</script></html>", "sem JavaScript");
        assert!(!erros.is_empty());
        let erros = violacoes_politica("<html><style>body{}</style></html>", "sem JavaScript");
        assert!(erros.is_empty());
    }

    #[test]
    fn workspace_bloqueia_parent_dir() {
        let root = std::env::temp_dir().join(format!("c2h-jail-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let canon = std::fs::canonicalize(&root).unwrap();
        let ws = Workspace {
            root: root.clone(),
            root_canon: canon,
            cwd: PathBuf::new(),
        };
        assert!(ws.resolver("../fora.rs", false).is_err());
        assert!(ws.resolver("sub/../../fora.rs", false).is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extrai_imports_html_css() {
        let html = "<link href=\"a.css\" rel=\"stylesheet\">\n<script src=\"a.js\"></script>";
        let imports = importacoes(html, "html");
        assert!(imports.contains(&"a.css".to_string()));
        assert!(imports.contains(&"a.js".to_string()));
        let css = "@import \"base.css\";\nbody { background: url(\"fundo.png\"); }";
        let imports = importacoes(css, "css");
        assert!(imports.contains(&"base.css".to_string()));
        assert!(imports.contains(&"fundo.png".to_string()));
    }

    #[test]
    fn validar_novo_rejeita_script_quando_pedido_sem_js() {
        let root = std::env::temp_dir().join(format!("c2h-new-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let canon = std::fs::canonicalize(&root).unwrap();
        let ws = Workspace {
            root: root.clone(),
            root_canon: canon,
            cwd: PathBuf::new(),
        };
        let alvo = root.join("a.html");
        let env = WriteEnvelope {
            path: "a.html".to_string(),
            content: "<html><script>alert(1)</script></html>".to_string(),
            imports: vec![],
            summary: "x".to_string(),
            w5h2: W5h2::default(),
        };
        assert!(validar_novo(&ws, &alvo, &env, "layout sem JS").is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extrai_json_de_fence() {
        let env = extrair_json("```json\n{\"contract_version\":1,\"changes\":[]}\n```").unwrap();
        assert_eq!(env.contract_version, 1);
    }

    #[test]
    fn rejeita_evidencia_com_linha_invalida() {
        let _cwd = CWD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "c2h-ev-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let anterior = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::fs::write("a.rs", "fn a() {}\n").unwrap();
        let env = ResponseEnvelope {
            contract_version: 1,
            changes: vec![Change {
                file: "a.rs".to_string(),
                block_id: "x".to_string(),
                action: "update".to_string(),
                new_code: "fn a() {}\n".to_string(),
                w5h2: W5h2 {
                    what: "a".to_string(),
                    ..W5h2::default()
                },
                evidence: vec!["a.rs:unknown".to_string()],
            }],
        };
        let violacoes = validar_resposta(Path::new("."), &env);
        assert!(violacoes.iter().any(|v| v.code == "E-EVID-02"));
        std::env::set_current_dir(anterior).unwrap();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejeita_path_traversal() {
        let env = ResponseEnvelope {
            contract_version: 1,
            changes: vec![Change {
                file: "../fora.rs".to_string(),
                block_id: "x".to_string(),
                action: "update".to_string(),
                new_code: "fn x() {}".to_string(),
                w5h2: W5h2::default(),
                evidence: vec![],
            }],
        };
        let violacoes = validar_resposta(Path::new("."), &env);
        assert!(violacoes.iter().any(|v| v.code == "E-PATH-01"));
    }

    #[test]
    fn aplica_update_no_bloco_e_gera_backup() {
        let _cwd = CWD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "c2h-apply-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let anterior = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::fs::write("a.rs", "fn a() {\n    let x = 1;\n}\n").unwrap();
        cmd_annotate(Path::new("a.rs"), false).unwrap();
        let env = ResponseEnvelope {
            contract_version: 1,
            changes: vec![Change {
                file: "a.rs".to_string(),
                block_id: "a.rs::fn::a".to_string(),
                action: "update".to_string(),
                new_code: "fn a() {\n    let x = 2;\n}\n".to_string(),
                w5h2: W5h2 {
                    what: "atualiza x".to_string(),
                    why: "teste".to_string(),
                    ..W5h2::default()
                },
                evidence: vec!["a.rs:1".to_string()],
            }],
        };
        aplicar_envelope(Path::new("."), &env, false).unwrap();
        let texto = std::fs::read_to_string("a.rs").unwrap();
        assert!(texto.contains("let x = 2"));
        let side = ler_sidecar(Path::new("a.rs")).unwrap();
        assert_eq!(side.blocks[0].w5h2.what, "atualiza x");
        std::env::set_current_dir(anterior).unwrap();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn extrai_section_html() {
        let blocos = extrair_blocos(
            Path::new("x.html"),
            "<section id=\"hero\">a</section>\n<section id=\"pricing\">b</section>\n",
        );
        assert_eq!(blocos.len(), 2);
        assert!(blocos.iter().any(|b| b.name == "pricing"));
    }
}

fn main() {
    let cli = Cli::parse();
    iniciar_debug(cli.debug);
    log_evento(
        "run_start",
        serde_json::json!({"command": nome_comando(cli.command.as_ref())}),
    );
    let resultado = match &cli.command {
        None | Some(Command::Chat) => cmd_chat(),
        Some(Command::Doctor) => cmd_doctor(),
        Some(Command::Blocks { file }) => cmd_blocks(file, cli.json),
        Some(Command::Annotate { file, dry_run, ai }) => {
            if *ai {
                cmd_annotate_ai(file, *dry_run)
            } else {
                cmd_annotate(file, *dry_run)
            }
        }
        Some(Command::Validate { file }) => cmd_validate(file.as_deref()),
        Some(Command::Status) => cmd_status(),
        Some(Command::Context { file, max_chars }) => cmd_context(file, *max_chars),
        Some(Command::Ask {
            prompt,
            system,
            file,
            apply,
            max_repairs,
        }) => cmd_ask(
            prompt,
            system.as_deref(),
            file.as_deref(),
            *apply,
            *max_repairs,
        ),
        Some(Command::ValidateResponse { response }) => cmd_validate_response(response),
        Some(Command::Apply { response, dry_run }) => cmd_apply(response, *dry_run),
        Some(Command::Logs { limit }) => cmd_logs(*limit),
        Some(Command::Fuzz { iters }) => cmd_fuzz(*iters),
        Some(Command::Config) => {
            cmd_config();
            Ok(())
        }
    };
    if let Err(erro) = resultado {
        eprintln!("c2h: {erro}");
        std::process::exit(1);
    }
}
