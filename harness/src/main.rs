#![recursion_limit = "512"]
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

static SHELL_ENABLED: AtomicBool = AtomicBool::new(false);
static BD_DISPONIVEL: AtomicBool = AtomicBool::new(false);
static GOAL_RUNNER_ENABLED: AtomicBool = AtomicBool::new(false);

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
    /// YOLO: nao pede confirmacao para escrita/rm/mv
    #[arg(long, global = true)]
    yolo: bool,
    /// Habilita shell livre (desligado por padrao)
    #[arg(long, global = true)]
    allow_shell: bool,
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
    goal: String,
    #[serde(default)]
    business_logic: String,
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

fn confidence_default() -> String {
    "medium".to_string()
}
fn verified_default() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SidecarBlock {
    id: String,
    kind: String,
    name: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    risk: String,
    #[serde(default = "confidence_default")]
    confidence: String,
    #[serde(default = "verified_default")]
    verified_by: String,
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
    let mut tools = serde_json::json!([
        {"type":"function","function":{"name":"list_dir","description":"Lista arquivos e diretorios do projeto.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"diretorio relativo; vazio para o atual"}}}}},
        {"type":"function","function":{"name":"read_file","description":"Le um arquivo de texto. Maximo 120 linhas por chamada; use full=true para arquivo inteiro.","parameters":{"type":"object","properties":{"path":{"type":"string"},"start":{"type":"integer"},"end":{"type":"integer"},"full":{"type":"boolean"}},"required":["path"]}}},
        {"type":"function","function":{"name":"find_text","description":"Busca texto nos arquivos do projeto.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}},
        {"type":"function","function":{"name":"write_file","description":"Cria SOMENTE arquivo novo. Se existir, use replace_file. O harness valida, mostra preview e pede confirmacao.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"summary":{"type":"string"}},"required":["path","content"]}}},
        {"type":"function","function":{"name":"replace_file","description":"Substitui arquivo existente com backup, tmp, validacao e rename atomico. Nao use remove_file para editar.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"summary":{"type":"string"}},"required":["path","content"]}}},
        {"type":"function","function":{"name":"edit_file","description":"Edita um bloco existente. Exige why_review confrontando o why atual do sidecar; patch validado e test gate.","parameters":{"type":"object","properties":{"path":{"type":"string"},"block_id":{"type":"string"},"new_code":{"type":"string"},"why_review":{"type":"string","description":"como a edicao preserva/atualiza o why atual do bloco"},"what":{"type":"string"},"why":{"type":"string"}},"required":["path","block_id","new_code","why_review"]}}},
        {"type":"function","function":{"name":"new_component","description":"Cria scaffold HTML/CSS/JS de um componente.","parameters":{"type":"object","properties":{"name":{"type":"string"},"html":{"type":"boolean"},"css":{"type":"boolean"},"js":{"type":"boolean"}},"required":["name"]}}},
        {"type":"function","function":{"name":"move_file","description":"Move um arquivo com backup.","parameters":{"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"]}}},
        {"type":"function","function":{"name":"remove_file","description":"Remove um arquivo com backup. NAO use para substituir conteudo; use replace_file.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}},
        {"type":"function","function":{"name":"run_test","description":"Roda comando configurado em .c2h/commands.json (test/test_all/build) ou C2H_TEST_CMD.","parameters":{"type":"object","properties":{"name":{"type":"string","description":"test, test_all, build; padrao test"}}}}},
        {"type":"function","function":{"name":"run_script","description":"Roda comando nomeado de .c2h/commands.json (ex: smoke-live).","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}},
        {"type":"function","function":{"name":"bd","description":"Executa o binario bd (beads) no root do projeto, sem shell. Subcomandos de escrita pedem confirmacao.","parameters":{"type":"object","properties":{"args":{"type":"array","items":{"type":"string"},"description":"argumentos do bd, por exemplo [\"ready\"] ou [\"create\",\"titulo\"]"}},"required":["args"]}}},
        {"type":"function","function":{"name":"shell","description":"PERIGO: executa comando de shell no root. Use apenas para testes/build/git; NAO leia codigo com type/cat/findstr/grep nem redirecione > para codigo. Pede confirmacao, exceto em YOLO.","parameters":{"type":"object","properties":{"command":{"type":"string","description":"comando completo"},"timeout_seconds":{"type":"integer","description":"timeout em segundos; padrao 60"}},"required":["command"]}}},
        {"type":"function","function":{"name":"git","description":"Executa o binario git instalado, sem shell, no root do projeto. Leitura automatica; clone/pull/commit/etc pedem confirmacao.","parameters":{"type":"object","properties":{"args":{"type":"array","items":{"type":"string"},"description":"argumentos do git, ex: [\"status\"] ou [\"clone\",\"--depth\",\"1\",\"https://...\",\"destino\"]"}},"required":["args"]}}},
        {"type":"function","function":{"name":"annotate_blocks","description":"Atualiza o W5H2 de blocos existentes SEM alterar codigo. Use why teleologico: proposito/causa/consequencia, nao mecanica.","parameters":{"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object","properties":{"block_id":{"type":"string"},"path":{"type":"string"},"start_line":{"type":"integer"},"what":{"type":"string"},"why":{"type":"string"},"goal":{"type":"string"},"business_logic":{"type":"string"},"when":{"type":"string"},"where":{"type":"string"},"who":{"type":"string"},"how":{"type":"string"},"how_much":{"type":"string"},"risk":{"type":"string","enum":["high","medium","low"]},"confidence":{"type":"string","enum":["high","medium","low"]},"verified_by":{"type":"string"},"evidence":{"type":"array","items":{"type":"string"}}},"required":["what","why"]}}},"required":["blocks"]}}},
        {"type":"function","function":{"name":"goal_status","description":"Mostra o goal ativo: objetivo, epic, status, tarefa atual, iteracoes e falhas.","parameters":{"type":"object","properties":{}}}},
        {"type":"function","function":{"name":"goal_tasks","description":"Lista tarefas do goal/epic com status e readiness.","parameters":{"type":"object","properties":{"status":{"type":"string","enum":["open","closed","all"],"description":"open (padrao), closed ou all"},"ready":{"type":"boolean","description":"somente prontas"}}}}},
        {"type":"function","function":{"name":"goal_next","description":"Mostra a proxima tarefa pronta do goal.","parameters":{"type":"object","properties":{}}}},
        {"type":"function","function":{"name":"goal_context","description":"Contexto do goal ativo e da tarefa atual, com eventos recentes.","parameters":{"type":"object","properties":{}}}},
        {"type":"function","function":{"name":"goal_events","description":"Eventos recentes do goal ativo.","parameters":{"type":"object","properties":{"limit":{"type":"integer","description":"padrao 10, max 100"}}}}}
    ]);
    if !SHELL_ENABLED.load(Ordering::Relaxed) {
        if let Some(lista) = tools.as_array_mut() {
            lista.retain(|t| t["function"]["name"].as_str() != Some("shell"));
        }
    }
    tools
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
    if AUTO_CONFIRM.load(Ordering::Relaxed) {
        println!("[yolo] {pergunta} -> sim");
        return Ok(true);
    }
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
    if linhas > 150 && env::var("C2H_SUGGEST_SPLIT").is_ok() {
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
    let _ = ler_sidecar(Path::new(&ws.rel(&alvo)));
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
        let html = format!("<!-- generated by C2 Harness -->\n<!doctype html>\n<html lang=\"pt-BR\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{nome}</title>\n{link}</head>\n<body>\n  <section data-c2-block=\"{slug}\">\n    <h1>{nome}</h1>\n  </section>\n</body>\n</html>\n");
        let path = dir.join(format!("{slug}.html"));
        escrever_atomico(&path, &html)?;
        criados.push(path);
    }
    if quer_css {
        let css = format!(
            "/* generated by C2 Harness */\n/* {nome} */\n.{slug} {{\n  display: block;\n}}\n"
        );
        let path = dir.join(format!("{slug}.css"));
        escrever_atomico(&path, &css)?;
        criados.push(path);
    }
    if quer_js {
        let js = format!("// generated by C2 Harness\n// {nome}\n");
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
                .map(|s| format!(" [sidecar status={} blocks={}]", s.status, s.blocks.len()))
                .unwrap_or_default();
            saida.push_str(&format!("{nome}{status}\n"));
        }
    }
    Ok(saida)
}

fn texto_ler(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let entrada = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let path = match ws.resolver(entrada, true) {
        Ok(p) => p,
        Err(erro) => {
            let nomes = nomes_do_cwd(ws);
            return Err(format!(
                "{erro}\nARQUIVOS DISPONIVEIS EM /{}: {}",
                ws.cwd.to_string_lossy().replace('\\', "/"),
                nomes.join(", ")
            ));
        }
    };
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
    let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);
    let fim_pedido = args.get("end").and_then(|v| v.as_u64()).unwrap_or(if full {
        linhas.len() as u64
    } else {
        120
    });
    let ini = ini.min(linhas.len().max(1));
    let fim = if full {
        fim_pedido.min(linhas.len() as u64) as usize
    } else {
        let teto = ini.saturating_add(119);
        (fim_pedido as usize).min(linhas.len()).min(teto)
    };
    let fim = fim.max(ini).min(linhas.len());
    let total = linhas.len();
    let mut saida = String::new();
    saida.push_str(&format!(
        "ARQUIVO: {}\nTOTAL: {} linhas\nMOSTRANDO: {}-{}\n",
        ws.rel(&path),
        total,
        ini,
        fim
    ));
    if fim < total {
        saida.push_str("TRUNCADO: use start/end para continuar.\n");
    }
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
            let fortes: Vec<_> = side
                .blocks
                .iter()
                .filter(|b| why_tecnico(&b.w5h2.why))
                .take(20)
                .collect();
            if !fortes.is_empty() {
                saida.push_str("\nANOTACOES RELEVANTES (why tecnico):\n");
                for bloco in fortes {
                    let mut linha = format!(
                        "- {} [{}] linhas {}-{} — {} — {}",
                        bloco.id,
                        bloco.kind,
                        bloco.start_line,
                        bloco.end_line,
                        bloco.w5h2.what,
                        bloco.w5h2.why
                    );
                    if !bloco.w5h2.goal.trim().is_empty() {
                        linha.push_str(&format!(" — goal: {}", bloco.w5h2.goal));
                    }
                    if !bloco.w5h2.business_logic.trim().is_empty() {
                        linha
                            .push_str(&format!(" — business_logic: {}", bloco.w5h2.business_logic));
                    }
                    if !bloco.w5h2.how.trim().is_empty() {
                        linha.push_str(&format!(" — how: {}", bloco.w5h2.how));
                    }
                    saida.push_str(&linha);
                    saida.push('\n');
                }
            }
            let sem_why: Vec<_> = side
                .blocks
                .iter()
                .filter(|b| !why_tecnico(&b.w5h2.why))
                .take(30)
                .collect();
            if !sem_why.is_empty() {
                saida.push_str("\nBLOCOS SEM WHY (use annotate_blocks com este block_id):\n");
                for bloco in sem_why {
                    saida.push_str(&format!(
                        "- {} [{}] linhas {}-{}\n",
                        bloco.id, bloco.kind, bloco.start_line, bloco.end_line
                    ));
                }
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

fn comentario_assinatura(ext: &str) -> Option<&'static str> {
    match ext {
        "html" | "htm" | "md" | "markdown" => Some("<!-- generated by C2 Harness -->"),
        "css" => Some("/* generated by C2 Harness */"),
        "py" | "sh" | "yaml" | "yml" | "toml" => Some("# generated by C2 Harness"),
        "rs" | "js" | "jsx" | "ts" | "tsx" | "c" | "h" | "cpp" | "hpp" | "go" => {
            Some("// generated by C2 Harness")
        }
        _ => None,
    }
}

fn assinar_conteudo(path: &Path, conteudo: &str) -> String {
    if env::var("C2H_SIGN")
        .map(|v| v == "0" || v == "false")
        .unwrap_or(false)
    {
        return conteudo.to_string();
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let Some(assinatura) = comentario_assinatura(&ext) else {
        return conteudo.to_string();
    };
    if conteudo
        .lines()
        .take(3)
        .any(|l| l.contains("generated by C2 Harness"))
    {
        return conteudo.to_string();
    }
    format!("{assinatura}\n{conteudo}")
}

fn ferramenta_replace_file(
    ws: &Workspace,
    args: &serde_json::Value,
    auto: bool,
    tarefa: &str,
) -> Result<String, String> {
    let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let alvo = ws.resolver(rel, true)?;
    if !alvo.is_file() {
        return Err(format!("arquivo nao existe para replace: {rel}"));
    }
    let content = assinar_conteudo(&alvo, content);
    let envelope = WriteEnvelope {
        path: rel.to_string(),
        content: content.clone(),
        imports: vec![],
        summary: summary.to_string(),
        w5h2: W5h2 {
            what: summary.to_string(),
            why: "desconhecido".to_string(),
            ..W5h2::default()
        },
    };
    validar_novo(ws, &alvo, &envelope, tarefa)?;
    revisar_requisitos(&content, tarefa)?;
    println!(
        "\n--- replace proposto: {rel} ({} linhas) ---",
        content.lines().count()
    );
    for (i, linha) in content.lines().take(120).enumerate() {
        println!("{:>4}: {linha}", i + 1);
    }
    if !auto && !confirmar(&format!("substituir {rel}?"))? {
        return Ok("cancelado pelo usuario".to_string());
    }
    let antigo_sidecar = ler_sidecar(Path::new(rel));
    let backup_path = backup(ws, &alvo)?;
    escrever_atomico(&alvo, &content)?;
    cmd_annotate(Path::new(rel), false)?;
    if precisa_anotacao(Path::new(rel)) && anotar_blocos_ia(Path::new(rel)).is_err() {
        if let Some(mut old) = antigo_sidecar {
            old.status = "stale".to_string();
            old.source_hash = hash(&content);
            escrever_sidecar(Path::new(rel), &old)?;
        }
    }
    marcar_qualidade(Path::new(rel));
    log_evento(
        "replace_file",
        serde_json::json!({"path": rel, "lines": content.lines().count(), "backup": backup_path.display().to_string()}),
    );
    Ok(format!(
        "substituido: {rel} ({} linhas)",
        content.lines().count()
    ))
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
        return Err(format!("arquivo ja existe: {rel}; use replace_file"));
    }
    let content = assinar_conteudo(&alvo, content);
    let envelope = WriteEnvelope {
        path: rel.to_string(),
        content: content.clone(),
        imports: vec![],
        summary: summary.to_string(),
        w5h2: W5h2 {
            what: summary.to_string(),
            why: "desconhecido".to_string(),
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
    escrever_atomico(&alvo, &content)?;
    cmd_annotate(Path::new(rel), false)?;
    if precisa_anotacao(Path::new(rel)) {
        if let Err(erro) = anotar_blocos_ia(Path::new(&ws.rel(&alvo))) {
            println!("aviso: annotation pass falhou: {erro}");
        }
    }
    let _ = ler_sidecar(Path::new(rel));
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

fn palavras_relevantes(texto: &str) -> std::collections::BTreeSet<String> {
    texto
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|p| p.len() >= 4)
        .map(|p| p.to_string())
        .collect()
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
    let why_atual = ler_sidecar(Path::new(&rel))
        .and_then(|s| s.blocks.into_iter().find(|b| b.id == block_id))
        .map(|b| b.w5h2.why)
        .unwrap_or_default();
    let why_review = args
        .get("why_review")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if why_review.len() < 15 {
        return Err(format!(
            "E-WHY-REVIEW: antes de editar, confronte o why atual.\nBLOCO: {block_id}\nWHY ATUAL: {why_atual}\nEnvie why_review explicando por que a edicao preserva ou muda esse why."
        ));
    }
    if !why_tem_consequencia(&why_atual) {
        return Err(format!(
            "E-WHY-REVIEW: bloco sem why tecnico; use annotate_blocks antes de editar.\nBLOCO: {block_id}\nWHY ATUAL: {why_atual}"
        ));
    }
    let atual = palavras_relevantes(&why_atual);
    let review = palavras_relevantes(why_review);
    let comuns = atual.intersection(&review).count();
    if comuns < 2 && comuns * 2 < atual.len() {
        return Err(format!(
            "E-WHY-REVIEW: o why_review nao confronta o why atual.\nWHY ATUAL: {why_atual}\nReescreva citando a consequencia/problema que o bloco protege."
        ));
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

fn valor_toml(bruto: &str) -> Option<String> {
    let v = bruto.trim();
    if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
        return Some(
            v[1..v.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        );
    }
    if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 {
        return Some(v[1..v.len() - 1].to_string());
    }
    let sem_comment = v.split(" #").next().unwrap_or(v).trim();
    if sem_comment.is_empty() {
        None
    } else {
        Some(sem_comment.to_string())
    }
}

fn comando_toml(path: &Path, nome: &str) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let raw = raw.trim_start_matches('\u{feff}');
    let mut secao = false;
    for linha in raw.lines() {
        let l = linha.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') && l.ends_with(']') {
            secao = l[1..l.len() - 1].trim() == "commands";
            continue;
        }
        if !secao {
            continue;
        }
        let Some((chave, valor)) = l.split_once('=') else {
            continue;
        };
        if chave.trim() == nome {
            return valor_toml(valor);
        }
    }
    None
}

fn comando_configurado(ws: &Workspace, nome: &str) -> Result<String, String> {
    let toml_path = ws.root.join(".c2h/commands.toml");
    if let Some(cmd) = comando_toml(&toml_path, nome) {
        return Ok(cmd);
    }
    let json_path = ws.root.join(".c2h/commands.json");
    if let Ok(raw) = fs::read_to_string(&json_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let cmd = v
                .get("commands")
                .and_then(|c| c.get(nome))
                .or_else(|| v.get(nome))
                .and_then(|c| c.as_str());
            if let Some(c) = cmd {
                return Ok(c.to_string());
            }
        }
    }
    if nome == "test" || nome == "default" {
        if let Ok(c) = env::var("C2H_TEST_CMD") {
            return Ok(c);
        }
    }
    Err(format!(
        "comando '{nome}' nao configurado; crie .c2h/commands.toml com [commands]\n{nome} = \"...\" (ou commands.json)"
    ))
}

fn rodar_comando_configurado(ws: &Workspace, nome: &str, evento: &str) -> Result<String, String> {
    let comando = comando_configurado(ws, nome)?;
    #[cfg(windows)]
    let saida = std::process::Command::new("cmd")
        .args(["/C", &comando])
        .current_dir(&ws.root)
        .output();
    #[cfg(not(windows))]
    let saida = std::process::Command::new("sh")
        .args(["-c", &comando])
        .current_dir(&ws.root)
        .output();
    match saida {
        Ok(out) => {
            let codigo = out.status.code().unwrap_or(-1);
            log_evento(evento, serde_json::json!({"name": nome, "exit": codigo}));
            Ok(format!(
                "comando={nome}\nexit={codigo}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stdout)
                    .chars()
                    .take(7000)
                    .collect::<String>(),
                String::from_utf8_lossy(&out.stderr)
                    .chars()
                    .take(3000)
                    .collect::<String>()
            ))
        }
        Err(e) => Err(format!("executar {nome}: {e}")),
    }
}

fn ferramenta_test(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let nome = args.get("name").and_then(|v| v.as_str()).unwrap_or("test");
    rodar_comando_configurado(ws, nome, "run_test")
}

fn ferramenta_run_script(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let nome = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "run_script exige name".to_string())?;
    rodar_comando_configurado(ws, nome, "run_script")
}

fn resumir_args_tool(args: &serde_json::Value) -> serde_json::Value {
    let mut resumo = serde_json::Map::new();
    for chave in [
        "path", "file", "from", "to", "name", "query", "start", "end", "block_id",
    ] {
        if let Some(valor) = args.get(chave) {
            resumo.insert(chave.to_string(), valor.clone());
        }
    }
    if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
        resumo.insert("command_chars".to_string(), serde_json::json!(cmd.len()));
        resumo.insert(
            "command_head".to_string(),
            serde_json::json!(cmd.chars().take(120).collect::<String>()),
        );
    }
    if let Some(lista) = args.get("blocks").and_then(|v| v.as_array()) {
        resumo.insert("blocks_count".to_string(), serde_json::json!(lista.len()));
    }
    if let Some(lista) = args.get("args").and_then(|v| v.as_array()) {
        resumo.insert("args_count".to_string(), serde_json::json!(lista.len()));
        if let Some(primeiro) = lista.first().and_then(|v| v.as_str()) {
            resumo.insert("subcommand".to_string(), serde_json::json!(primeiro));
        }
    }
    for chave in ["content", "new_code"] {
        if let Some(texto) = args.get(chave).and_then(|v| v.as_str()) {
            resumo.insert(format!("{chave}_chars"), serde_json::json!(texto.len()));
            resumo.insert(format!("{chave}_hash"), serde_json::json!(hash(texto)));
        }
    }
    serde_json::Value::Object(resumo)
}

fn normalizar_path(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

fn resolver_block_id(
    block_id: &str,
    path: &str,
    start_line: Option<usize>,
    sidecars: &[Sidecar],
) -> Result<String, String> {
    let existe = |id: &str| sidecars.iter().any(|s| s.blocks.iter().any(|b| b.id == id));
    if !block_id.is_empty() && existe(block_id) {
        return Ok(block_id.to_string());
    }
    let mut alvo_path = path.to_string();
    let mut alvo_linha = start_line;
    if alvo_path.is_empty() {
        if let Some((pre, suf)) = block_id.rsplit_once(':') {
            if let Ok(n) = suf.parse::<usize>() {
                alvo_path = pre.to_string();
                alvo_linha = Some(n);
            }
        }
    }
    if !alvo_path.is_empty() {
        let alvo = normalizar_path(&alvo_path);
        for side in sidecars {
            let file = normalizar_path(&side.file);
            if file == alvo
                || file.ends_with(&format!("/{alvo}"))
                || alvo.ends_with(&format!("/{file}"))
            {
                if let Some(linha) = alvo_linha {
                    if let Some(b) = side.blocks.iter().find(|b| b.start_line == linha) {
                        return Ok(b.id.clone());
                    }
                }
            }
        }
    }
    let mut validos = Vec::new();
    for side in sidecars {
        let file = normalizar_path(&side.file);
        let mesmo = alvo_path.is_empty()
            || file == normalizar_path(&alvo_path)
            || file.ends_with(&format!("/{}", normalizar_path(&alvo_path)));
        if mesmo {
            for b in side.blocks.iter().take(20) {
                validos.push(b.id.clone());
            }
        }
        if validos.len() >= 20 {
            break;
        }
    }
    if validos.is_empty() {
        for side in sidecars {
            for b in side.blocks.iter().take(5) {
                validos.push(b.id.clone());
            }
        }
    }
    Err(format!(
        "block_id desconhecido: {block_id}\nIDs validos:\n- {}",
        validos
            .into_iter()
            .take(20)
            .collect::<Vec<_>>()
            .join("\n- ")
    ))
}

fn executar_annotate_blocks(args: &serde_json::Value) -> Result<String, String> {
    let lista = args
        .get("blocks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "use annotate_blocks({blocks:[...]})".to_string())?;
    if lista.is_empty() {
        return Err("blocks vazio".to_string());
    }
    let mut sidecars = sidecars_do_projeto();
    let mut updates: Vec<(String, W5h2, Vec<String>, String, String, String)> = Vec::new();
    let mut erros = Vec::new();
    for item in lista {
        let id_bruto = item.get("block_id").and_then(|v| v.as_str()).unwrap_or("");
        let path_arg = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let start_arg = item
            .get("start_line")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let what = item.get("what").and_then(|v| v.as_str()).unwrap_or("");
        let why = item.get("why").and_then(|v| v.as_str()).unwrap_or("");
        if what.trim().is_empty() || !why_tem_consequencia(why) {
            erros.push(format!("bloco invalido/why fraco: {id_bruto}"));
            continue;
        }
        let id = match resolver_block_id(id_bruto, path_arg, start_arg, &sidecars) {
            Ok(id) => id,
            Err(e) => {
                erros.push(e);
                continue;
            }
        };
        let w5h2 = W5h2 {
            what: what.to_string(),
            why: why.to_string(),
            goal: item
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            business_logic: item
                .get("business_logic")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            when: item
                .get("when")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            where_: item
                .get("where")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            who: item
                .get("who")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            how: item
                .get("how")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            how_much: item
                .get("how_much")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        };
        let ev = item
            .get("evidence")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let risk = item
            .get("risk")
            .and_then(|v| v.as_str())
            .unwrap_or("medium")
            .to_string();
        let confidence = item
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("medium")
            .to_string();
        let verified = item
            .get("verified_by")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string();
        updates.push((id.to_string(), w5h2, ev, risk, confidence, verified));
    }
    if !erros.is_empty() {
        return Err(format!("annotate_blocks rejeitado:\n{}", erros.join("\n")));
    }
    let mut total = 0usize;
    for side in &mut sidecars {
        for (id, w5h2, ev, risk, confidence, verified) in &updates {
            if let Some(bloco) = side.blocks.iter_mut().find(|b| &b.id == id) {
                bloco.w5h2 = w5h2.clone();
                if !ev.is_empty() {
                    bloco.evidence = ev.clone();
                }
                bloco.origin = "agent_annotation".to_string();
                bloco.risk = risk.clone();
                bloco.confidence = confidence.clone();
                bloco.verified_by = verified.clone();
                total += 1;
            }
        }
        if total > 0 {
            side.status = if side.blocks.iter().all(|b| why_tecnico(&b.w5h2.why)) {
                "annotated".to_string()
            } else {
                "unannotated".to_string()
            };
            escrever_sidecar(Path::new(&side.file), side)?;
        }
    }
    log_evento("annotate_blocks", serde_json::json!({"blocks": total}));
    Ok(format!("annotate_blocks: {total} blocos atualizados"))
}

fn executar_git(ws: &Workspace, args: &serde_json::Value, auto: bool) -> Result<String, String> {
    let lista: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if lista.is_empty() {
        return Err("uso: git({args:[\"status\"]})".to_string());
    }
    let sub = lista[0].trim_start_matches('-').to_lowercase();
    let leitura = [
        "status",
        "diff",
        "log",
        "show",
        "branch",
        "rev-parse",
        "remote",
        "tag",
        "ls-files",
        "version",
        "help",
        "blame",
        "describe",
    ];
    let escrita = [
        "clone",
        "pull",
        "fetch",
        "add",
        "commit",
        "checkout",
        "restore",
        "switch",
        "merge",
        "rebase",
        "reset",
        "clean",
        "push",
        "init",
        "mv",
        "rm",
        "stash",
        "submodule",
        "worktree",
    ];
    if !leitura.contains(&sub.as_str()) && !escrita.contains(&sub.as_str()) {
        return Err(format!("subcomando git nao permitido: {}", lista[0]));
    }
    let eh_escrita = escrita.contains(&sub.as_str()) && !leitura.contains(&sub.as_str());
    if eh_escrita && !auto && !confirmar(&format!("git {} ?", lista.join(" ")))? {
        return Ok("cancelado pelo usuario".to_string());
    }
    let saida = std::process::Command::new("git")
        .args(&lista)
        .current_dir(&ws.root)
        .output()
        .map_err(|e| format!("executar git: {e}. git esta no PATH?"))?;
    log_evento(
        "git_call",
        serde_json::json!({"sub": sub, "args": lista.len(), "exit": saida.status.code()}),
    );
    let texto = format!(
        "exit={}\nstdout:\n{}\nstderr:\n{}",
        saida.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );
    Ok(texto.chars().take(10000).collect())
}

fn extensao_codigo(path: &str) -> bool {
    let p = path.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
    [
        "rs", "py", "js", "jsx", "ts", "tsx", "html", "htm", "css", "md",
    ]
    .iter()
    .any(|ext| p.ends_with(&format!(".{ext}")))
}

fn alvo_redirecionamento(comando: &str) -> Option<String> {
    let bytes = comando.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'>' {
            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'>' {
                j += 1;
            }
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let inicio = j;
            while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b';' {
                j += 1;
            }
            let alvo = comando[inicio..j].trim_matches(|c| c == '"' || c == '\'');
            if extensao_codigo(alvo) {
                return Some(alvo.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

fn detectar_bypass_shell(comando: &str) -> Option<(&'static str, String)> {
    let c = comando.to_lowercase();
    let leitura = [
        "type ",
        "cat ",
        "more ",
        "findstr ",
        "grep ",
        "rg ",
        "get-content ",
        "select-string ",
        "head ",
        "tail ",
    ];
    if leitura.iter().any(|p| c.contains(p)) {
        for token in comando.split_whitespace() {
            let t = token.trim_matches(|ch| ch == '"' || ch == '\'' || ch == ',' || ch == ';');
            if extensao_codigo(t) {
                return Some(("read_code", t.to_string()));
            }
        }
    }
    if let Some(alvo) = alvo_redirecionamento(comando) {
        return Some(("write_code", alvo));
    }
    None
}

fn eh_teste_ou_build(comando: &str) -> bool {
    let c = comando.trim().to_lowercase();
    [
        "npm test",
        "npm run test",
        "npm run build",
        "npm run lint",
        "cargo test",
        "cargo build",
        "cargo check",
        "cargo clippy",
        "pytest",
        "go test",
        "go build",
        "dotnet test",
        "dotnet build",
        "make test",
        "make build",
        "node --test",
        "node tools/smoke",
        "node tools\\smoke",
        "node tools/draw-cli",
        "node tools\\draw-cli",
    ]
    .iter()
    .any(|p| c.starts_with(p) || c.contains(&format!("&& {p}")) || c.contains(&format!("& {p}")))
}

fn classe_shell(comando: &str) -> &'static str {
    if let Some(("read_code", _)) = detectar_bypass_shell(comando) {
        "read_code"
    } else if let Some(("write_code", _)) = detectar_bypass_shell(comando) {
        "write_code"
    } else if eh_teste_ou_build(comando) {
        "test_build"
    } else {
        "unknown"
    }
}

fn executar_shell(ws: &Workspace, args: &serde_json::Value, auto: bool) -> Result<String, String> {
    if !SHELL_ENABLED.load(Ordering::Relaxed) {
        return Err(
            "E-SHELL-DISABLED: shell desligado. Use run_test/run_script ou abra com --allow-shell."
                .to_string(),
        );
    }
    use std::io::Read;
    use std::sync::mpsc;
    use std::time::Duration;
    let comando = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if comando.trim().is_empty() {
        return Err("uso: shell({command: \"git status\"})".to_string());
    }
    if let Some((kind, file)) = detectar_bypass_shell(&comando) {
        log_evento(
            "shell_bypass",
            serde_json::json!({"kind": kind, "file": file, "blocked": true, "head": comando.chars().take(120).collect::<String>()}),
        );
        if kind == "read_code" {
            return Err(format!(
                "E-SHELL-BYPASS: leitura de codigo por shell bloqueada: {file}. Use read_file(path=\"{file}\") ou find_text."
            ));
        }
        return Err(format!(
            "E-SHELL-BYPASS: escrita de codigo por shell bloqueada: {file}. Use replace_file/write_file."
        ));
    }
    let timeout = args
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(60)
        .clamp(1, 600);
    if !auto && !confirmar(&format!("executar shell: {comando} ?"))? {
        return Ok("cancelado pelo usuario".to_string());
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
    let mut filho = cmd
        .current_dir(&ws.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn shell: {e}"))?;
    let mut stdout = filho.stdout.take();
    let mut stderr = filho.stderr.take();
    let (tx_out, rx_out) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();
    if let Some(mut s) = stdout.take() {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            let _ = tx_out.send(buf);
        });
    }
    if let Some(mut s) = stderr.take() {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            let _ = tx_err.send(buf);
        });
    }
    let inicio = std::time::Instant::now();
    let codigo = loop {
        if let Ok(Some(s)) = filho.try_wait() {
            break s.code().unwrap_or(-1);
        }
        if inicio.elapsed() > Duration::from_secs(timeout) {
            let _ = filho.kill();
            break filho.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let out = rx_out
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    let err = rx_err
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    let texto = format!(
        "exit={codigo}\nstdout:\n{}\nstderr:\n{}",
        out.chars().take(6000).collect::<String>(),
        err.chars().take(6000).collect::<String>()
    );
    log_evento(
        "shell_call",
        serde_json::json!({"command_chars": comando.len(), "head": comando.chars().take(80).collect::<String>(), "exit": codigo, "class": classe_shell(&comando), "timeout": inicio.elapsed().as_secs() >= timeout}),
    );
    Ok(texto)
}

fn executar_bd(ws: &Workspace, args: &serde_json::Value, auto: bool) -> Result<String, String> {
    let lista: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if lista.is_empty() {
        return Err("uso: bd({args:[\"ready\"]})".to_string());
    }
    let sub = lista[0].trim_start_matches('-').to_lowercase();
    let leitura = [
        "ready", "list", "show", "status", "prime", "export", "help", "version",
    ];
    let escrita = [
        "init", "create", "update", "close", "delete", "remember", "dep", "sync", "import",
        "export",
    ];
    if !leitura.contains(&sub.as_str()) && !escrita.contains(&sub.as_str()) {
        return Err(format!("subcomando bd nao permitido: {}", lista[0]));
    }
    let eh_escrita = escrita.contains(&sub.as_str()) && !leitura.contains(&sub.as_str());
    if eh_escrita && !auto && !confirmar(&format!("bd {} ?", lista.join(" ")))? {
        return Ok("cancelado pelo usuario".to_string());
    }
    let saida = std::process::Command::new("bd")
        .args(&lista)
        .current_dir(&ws.root)
        .output()
        .map_err(|e| format!("executar bd: {e}. bd esta no PATH?"))?;
    log_evento(
        "bd_call",
        serde_json::json!({"sub": sub, "args": lista.len(), "exit": saida.status.code()}),
    );
    let texto = format!(
        "exit={}\nstdout:\n{}\nstderr:\n{}",
        saida.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );
    Ok(texto.chars().take(8000).collect())
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
    log_evento(
        "tool_call",
        serde_json::json!({"name": nome, "args": resumir_args_tool(&args)}),
    );
    let resultado = match nome {
        "list_dir" => texto_listar(ws, &args),
        "read_file" => texto_ler(ws, &args),
        "find_text" => texto_buscar(ws, &args),
        "write_file" => ferramenta_write(ws, &args, auto, tarefa),
        "replace_file" => ferramenta_replace_file(ws, &args, auto, tarefa),
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
        "run_test" => ferramenta_test(ws, &args),
        "run_script" => ferramenta_run_script(ws, &args),
        "bd" => executar_bd(ws, &args, auto),
        "shell" => executar_shell(ws, &args, auto),
        "git" => executar_git(ws, &args, auto),
        "annotate_blocks" => executar_annotate_blocks(&args),
        "goal_status" => ferramenta_goal_status(ws),
        "goal_tasks" => ferramenta_goal_tasks(ws, &args),
        "goal_next" => ferramenta_goal_next(ws),
        "goal_context" => ferramenta_goal_context(ws),
        "goal_events" => ferramenta_goal_events(ws, &args),
        outro => Err(format!("ferramenta desconhecida: {outro}")),
    };
    log_evento(
        "tool_result",
        serde_json::json!({"name": nome, "ok": resultado.is_ok()}),
    );
    resultado
}

fn descrever_tool(nome: &str, argumentos: &str) -> String {
    let Ok(args) = serde_json::from_str::<serde_json::Value>(argumentos) else {
        return nome.to_string();
    };
    match nome {
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let cmd = cmd.replace('\n', " ").chars().take(180).collect::<String>();
            format!("shell: {cmd}")
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let start = args.get("start").and_then(|v| v.as_u64());
            let end = args.get("end").and_then(|v| v.as_u64());
            match (start, end) {
                (Some(a), Some(b)) => format!("read_file: {path} ({a}-{b})"),
                (Some(a), None) => format!("read_file: {path} ({a}-)"),
                _ => format!("read_file: {path}"),
            }
        }
        "write_file" | "replace_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("{nome}: {path}")
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let block = args.get("block_id").and_then(|v| v.as_str()).unwrap_or("?");
            format!("edit_file: {path} :: {block}")
        }
        "bd" | "git" => {
            let lista = args
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .take(5)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("{nome}: {lista}")
        }
        "annotate_blocks" => {
            let n = args
                .get("blocks")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            format!("annotate_blocks: {n} blocos")
        }
        "find_text" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("?");
            format!("find_text: {q}")
        }
        _ => nome.to_string(),
    }
}

fn executar_agente(
    system: &str,
    historico: &mut Vec<serde_json::Value>,
    ws: &Workspace,
    auto: bool,
) -> Result<(), String> {
    let tools = ferramentas();
    let max_passos = env::var("C2H_MAX_STEPS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(24)
        .clamp(2, 64);
    let reserva = env::var("C2H_RESERVE_STEPS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .min(max_passos.saturating_sub(1));
    let mut cache_leituras: BTreeMap<String, String> = BTreeMap::new();
    let mut leituras_por_arquivo: BTreeMap<String, usize> = BTreeMap::new();
    let tarefa = historico
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user"))
        .and_then(|m| m.get("content").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    for passo in 0..max_passos {
        let mut mensagens = vec![serde_json::json!({"role":"system","content":system})];
        mensagens.extend(historico.clone());
        if passo + 1 == max_passos.saturating_sub(reserva) {
            mensagens.push(serde_json::json!({
                "role": "user",
                "content": format!("ORCAMENTO: restam {reserva} passos. Pare de explorar/anotar e priorize write_file/replace_file/edit_file + run_test.")
            }));
        }
        if passo + 1 == max_passos {
            mensagens.push(serde_json::json!({
                "role": "user",
                "content": "ORCAMENTO DE EXPLORACAO ESGOTADO. Nao chame ferramentas. Responda AGORA com o plano, as conclusoes e as lacunas usando apenas o que ja coletou."
            }));
            log_evento("agent_forced_final", serde_json::json!({"step": passo + 1}));
            let msg = chamar_api(&mensagens, None)?;
            let content = msg
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            println!("\n{content}\n");
            historico.push(msg);
            return Ok(());
        }
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
        let mut msg = msg;
        if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
            if !content.trim().is_empty() {
                log_evento(
                    "assistant_preamble_ignored",
                    serde_json::json!({"chars": content.len()}),
                );
            }
        }
        if let Some(obj) = msg.as_object_mut() {
            obj.insert("content".to_string(), serde_json::Value::Null);
        }
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
            println!("  -> {}", descrever_tool(nome, argumentos));
            if nome == "read_file" {
                let chave = format!("read_file|{argumentos}");
                if cache_leituras.contains_key(&chave) {
                    let aviso = "LEITURA REPETIDA: este arquivo/faixa ja foi lido neste turno. Use o resultado anterior ou o contexto do harness.".to_string();
                    log_evento(
                        "tool_repeat",
                        serde_json::json!({"name": nome, "reason": "same_args"}),
                    );
                    historico
                        .push(serde_json::json!({"role":"tool","tool_call_id":id,"content":aviso}));
                    continue;
                }
                let path_lido = serde_json::from_str::<serde_json::Value>(argumentos)
                    .ok()
                    .and_then(|v| {
                        v.get("path")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                let contador = leituras_por_arquivo.entry(path_lido.clone()).or_insert(0);
                if *contador >= 2 {
                    let aviso = format!("LIMITE: no maximo 2 leituras por arquivo neste turno ({path_lido}). Use o contexto ja enviado ou peca uma faixa especifica em outro turno.");
                    log_evento(
                        "tool_repeat",
                        serde_json::json!({"name": nome, "reason": "limit", "file": path_lido}),
                    );
                    historico
                        .push(serde_json::json!({"role":"tool","tool_call_id":id,"content":aviso}));
                    continue;
                }
                *contador += 1;
            }
            let resultado = match executar_ferramenta(ws, auto, &tarefa, nome, argumentos) {
                Ok(texto) => texto,
                Err(erro) => format!("ERRO DA FERRAMENTA: {erro}"),
            };
            if nome == "read_file" {
                cache_leituras.insert(format!("read_file|{argumentos}"), resultado.clone());
            }
            let truncado: String = if resultado.chars().count() > 12000 {
                format!(
                    "{}\n[TRUNCADO PELO HARNESS; use argumentos mais especificos]",
                    resultado.chars().take(12000).collect::<String>()
                )
            } else {
                resultado
            };
            log_evento(
                "tool_result_preview",
                serde_json::json!({"name": nome, "preview": truncado.chars().take(240).collect::<String>()}),
            );
            historico.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": truncado
            }));
        }
    }
    Err(format!("agente excedeu {max_passos} passos"))
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
        .filter(|s| Path::new(&s.file).is_file())
        .collect()
}

fn risco_heuristico(nome: &str, kind: &str) -> String {
    let n = nome.to_lowercase();
    let critico = [
        "format",
        "load",
        "loaddoc",
        "save",
        "savedoc",
        "parse",
        "encode",
        "decode",
        "write",
        "read",
        "validate",
        "header",
        "meta",
        "trim",
        "serialize",
        "deserialize",
        "import",
        "export",
        "sync",
        "persist",
    ];
    if kind == "section" || kind == "block" {
        return "medium".to_string();
    }
    if critico.iter().any(|c| n.contains(c)) {
        "high".to_string()
    } else if n.contains("render") || n.contains("draw") || n.contains("ui") {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

fn why_tecnico(texto: &str) -> bool {
    let t = texto.trim().to_lowercase();
    if t.len() < 15 {
        return false;
    }
    let genericos = [
        "solicitado pelo agente",
        "necessario",
        "necessário",
        "melhor pratica",
        "melhor prática",
        "desconhecido",
        "unknown",
        "n/a",
        "-",
    ];
    !genericos.iter().any(|g| t == *g || t.starts_with(g))
}

fn why_tem_consequencia(texto: &str) -> bool {
    if !why_tecnico(texto) {
        return false;
    }
    let t = texto.trim().to_lowercase();
    let mecanico = [
        "chama ",
        "registra ",
        "inicializa ",
        "retorna ",
        "usa ",
        "faz ",
    ];
    if mecanico.iter().any(|p| t.starts_with(p)) {
        return false;
    }
    let consequencia = [
        "porque", "pois", "sem isso", "sem ", "evita", "impede", "garante", "corrompe", "perde",
        "quebra", "falha", "erro", "senão", "senao", "para que", "a fim de",
    ];
    consequencia.iter().any(|p| t.contains(p))
}

fn path_anotavel(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    let proibidos = [
        "tmp/",
        "/tmp/",
        "target/",
        "dist/",
        "node_modules/",
        "vendor/",
        "generated/",
        ".git/",
        ".c2h/",
    ];
    if proibidos.iter().any(|x| p.contains(x)) {
        return false;
    }
    let nome = p.rsplit('/').next().unwrap_or(&p);
    !(nome.starts_with("probe-") || nome.starts_with("fix-") || nome.ends_with(".min.js"))
}

fn arquivo_codigo(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "html" | "htm" | "css" | "md"
    )
}

fn coletar_codigo(dir: &Path, nivel: usize, out: &mut Vec<String>) {
    if nivel > 3 || out.len() >= 300 {
        return;
    }
    let Ok(entradas) = fs::read_dir(dir) else {
        return;
    };
    for item in entradas.filter_map(|e| e.ok()) {
        let path = item.path();
        let rel = path.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            if !path_anotavel(&format!("{rel}/")) {
                continue;
            }
            coletar_codigo(&path, nivel + 1, out);
        } else if arquivo_codigo(&path) && path_anotavel(&rel) {
            out.push(rel);
        }
        if out.len() >= 300 {
            return;
        }
    }
}

fn arquivos_criticos_sem_anotacao(ws: &Workspace, ativo: Option<&str>) -> Vec<String> {
    let mut candidatos = std::collections::BTreeSet::new();
    if let Some(a) = ativo {
        candidatos.insert(a.to_string());
    }
    if let Ok(saida) = std::process::Command::new("git")
        .args(["diff", "--name-only"])
        .output()
    {
        for linha in String::from_utf8_lossy(&saida.stdout).lines() {
            if arquivo_codigo(Path::new(linha.trim())) {
                candidatos.insert(linha.trim().to_string());
            }
        }
    }
    let mut locais = Vec::new();
    coletar_codigo(&ws.cwd_abs(), 0, &mut locais);
    for l in locais {
        candidatos.insert(l);
    }
    candidatos
        .into_iter()
        .filter(|rel| {
            let side = ler_sidecar(Path::new(rel));
            side.map(|s| !s.blocks.iter().any(|b| why_tecnico(&b.w5h2.why)))
                .unwrap_or(true)
        })
        .take(8)
        .collect()
}

fn extrair_comentarios(conteudo: &str, max_linhas: usize) -> String {
    let mut saida = String::new();
    let mut usadas = 0usize;
    for (i, linha) in conteudo.lines().enumerate() {
        let t = linha.trim();
        let comentario = t.starts_with("//")
            || t.starts_with("#")
            || t.starts_with("/*")
            || t.starts_with('*')
            || t.starts_with("--");
        if comentario {
            saida.push_str(&format!("{}: {}\n", i + 1, t));
            usadas += 1;
            if usadas >= max_linhas {
                break;
            }
        }
    }
    saida
}

fn nomes_do_cwd(ws: &Workspace) -> Vec<String> {
    let mut nomes: Vec<String> = fs::read_dir(ws.cwd_abs())
        .map(|itens| {
            itens
                .filter_map(|e| e.ok())
                .map(|e| {
                    let nome = e.file_name().to_string_lossy().to_string();
                    let dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
                    if dir {
                        format!("{nome}/")
                    } else {
                        nome
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    nomes.sort();
    nomes.truncate(60);
    nomes
}

fn shell_info() -> (&'static str, &'static str) {
    #[cfg(windows)]
    {
        (
            "Windows",
            "cmd /C (use where, dir, type, echo; NAO use which, command -v, for ...; do ...; done, printf)",
        )
    }
    #[cfg(not(windows))]
    {
        (
            std::env::consts::OS,
            "sh -c (use which, command -v, for, printf, etc.)",
        )
    }
}

fn compilar_contexto(ws: &Workspace, tarefa: &str, ativo: Option<&str>) -> String {
    let tokens: Vec<String> = tarefa
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect();
    let cwd_rel = ws.cwd.to_string_lossy().replace('\\', "/");
    let sidecars = sidecars_do_projeto();
    let changed: std::collections::BTreeSet<String> = std::process::Command::new("git")
        .args(["diff", "--name-only"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let imports_ativo: std::collections::BTreeSet<String> = ativo
        .and_then(|a| fs::read_to_string(a).ok())
        .map(|texto| {
            importacoes(
                &texto,
                Path::new(ativo.unwrap())
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or(""),
            )
            .into_iter()
            .collect()
        })
        .unwrap_or_default();
    let mut candidatos: Vec<(usize, String, String, String)> = Vec::new();
    let mut stale = 0usize;
    let mut sem_why = 0usize;
    for side in &sidecars {
        let Ok(fonte) = fs::read_to_string(&side.file) else {
            continue;
        };
        let linhas: Vec<&str> = fonte.lines().collect();
        for bloco in &side.blocks {
            if bloco.start_line == 0
                || bloco.end_line < bloco.start_line
                || bloco.end_line > linhas.len()
            {
                stale += 1;
                continue;
            }
            if !why_tecnico(&bloco.w5h2.why) {
                sem_why += 1;
                continue;
            }
            if bloco.confidence == "low" {
                stale += 1;
                continue;
            }
            let corpo = normalizar_conteudo(&linhas[bloco.start_line - 1..bloco.end_line]);
            if hash(&corpo) != bloco.semantic_hash {
                stale += 1;
                continue;
            }
            let mut score = 0usize;
            if ativo == Some(side.file.as_str()) {
                score += 1000;
            }
            if !cwd_rel.is_empty() && side.file.starts_with(&cwd_rel) {
                score += 100;
            }
            if changed.contains(&side.file) {
                score += 200;
            }
            if imports_ativo
                .iter()
                .any(|imp| imp.contains(&side.file) || side.file.ends_with(imp))
            {
                score += 100;
            }
            if bloco.risk == "high" {
                score += 80;
            } else if bloco.risk == "medium" {
                score += 30;
            }
            let texto =
                format!("{} {} {}", bloco.id, bloco.w5h2.what, bloco.w5h2.why).to_lowercase();
            score += tokens.iter().filter(|t| texto.contains(t.as_str())).count() * 25;
            score += (bloco.w5h2.why.len() / 20).min(30);
            if why_tem_consequencia(&bloco.w5h2.why) {
                score += 40;
            }
            let mut linha = format!(
                "- {}:{} — {} — {}",
                side.file,
                bloco.start_line,
                bloco.w5h2.what.chars().take(160).collect::<String>(),
                bloco.w5h2.why.chars().take(260).collect::<String>()
            );
            linha.push_str(&format!(
                " [conf:{} verified:{}]",
                bloco.confidence, bloco.verified_by
            ));
            if !bloco.w5h2.how.trim().is_empty() {
                linha.push_str(&format!(
                    " — how: {}",
                    bloco.w5h2.how.chars().take(120).collect::<String>()
                ));
            }
            candidatos.push((score, side.file.clone(), bloco.risk.clone(), linha));
        }
    }
    candidatos.sort_by_key(|(score, _, _, _)| std::cmp::Reverse(*score));

    let mut out = String::new();
    out.push_str("<contexto_do_harness>\n");
    out.push_str(&format!("PROJETO: {}\n", ws.root.display()));
    out.push_str(&format!("CWD: /{cwd_rel}\n"));
    let (sistema, shell) = shell_info();
    let shell = if SHELL_ENABLED.load(Ordering::Relaxed) {
        shell.to_string()
    } else {
        "desabilitado (use run_test/run_script; /shell on ou --allow-shell)".to_string()
    };
    out.push_str(&format!("SISTEMA OPERACIONAL: {sistema}\n"));
    out.push_str(&format!("SHELL DA TOOL: {shell}\n"));
    let nomes = nomes_do_cwd(ws);
    if !nomes.is_empty() {
        out.push_str("ARQUIVOS NO CWD:\n");
        for nome in &nomes {
            out.push_str(&format!("- {nome}\n"));
        }
    }
    if let Some(ativo) = ativo {
        out.push_str(&format!("ARQUIVO ATIVO: {ativo}\n"));
        if let Ok(texto) = fs::read_to_string(ativo) {
            let ext = Path::new(ativo)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let imports = importacoes(&texto, ext);
            if !imports.is_empty() {
                out.push_str("IMPORTS DO ARQUIVO ATIVO:\n");
                for imp in imports.iter().take(20) {
                    out.push_str(&format!("- {imp}\n"));
                }
            }
        }
    }

    if BD_DISPONIVEL.load(Ordering::Relaxed) && GOAL_RUNNER_ENABLED.load(Ordering::Relaxed) {
        if let Some(goal) = goal_ativo(ws) {
            if let Ok(tarefas) = goal_tarefas_bd(ws, &goal.epic) {
                let ready = goal_ready_ids(ws, &goal.epic);
                let atual = goal_tarefa_atual(ws, &goal, &tarefas, &ready);
                let restantes = tarefas
                    .iter()
                    .filter(|t| t.get("status").and_then(|v| v.as_str()) != Some("closed"))
                    .count();
                out.push_str("\nGOAL ATIVO:\n");
                out.push_str(&format!(
                    "- id: {}\n- objetivo: {}\n- epic: {}\n- status: {}\n",
                    goal.id, goal.objective, goal.epic, goal.status
                ));
                if let Some(t) = atual {
                    out.push_str(&format!(
                        "- tarefa atual: {} — {}\n",
                        t.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
                        t.get("title").and_then(|v| v.as_str()).unwrap_or("")
                    ));
                } else {
                    out.push_str("- tarefa atual: nenhuma\n");
                }
                out.push_str(&format!("- tarefas restantes: {restantes}\n"));
            }
        }
    }

    let criticos = arquivos_criticos_sem_anotacao(ws, ativo);
    if !criticos.is_empty() {
        out.push_str("\nARQUIVOS CRITICOS SEM ANOTACAO:\n");
        for c in &criticos {
            out.push_str(&format!("- {c}\n"));
        }
    }

    let mut vistos = std::collections::BTreeSet::new();
    let mut arquivos = 0usize;
    for (_, file, _, _) in &candidatos {
        if arquivos >= 2 || !vistos.insert(file.clone()) {
            continue;
        }
        if let Ok(fonte) = fs::read_to_string(file) {
            let comentarios = extrair_comentarios(&fonte, 12);
            if !comentarios.trim().is_empty() {
                out.push_str(&format!("\nCOMENTARIOS ({file}):\n{comentarios}"));
                arquivos += 1;
            }
        }
    }

    let mut blocos = 0usize;
    let mut bytes_blocos = 0usize;
    let mut arquivos_blocos = std::collections::BTreeSet::new();
    out.push_str("\nBLOCOS RELEVANTES (why tecnico + hash valido):\n");
    for (_, file, _, linha) in &candidatos {
        if bytes_blocos + linha.len() > 5000 || blocos >= 40 {
            break;
        }
        bytes_blocos += linha.len();
        blocos += 1;
        arquivos_blocos.insert(file.clone());
        out.push_str(linha);
        out.push('\n');
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
        serde_json::json!({
            "chars": truncado.len(),
            "files": arquivos_blocos.len(),
            "blocks": blocos,
            "stale_skipped": stale,
            "empty_why_skipped": sem_why,
            "critical_unannotated": criticos.len(),
            "active": ativo
        }),
    );
    truncado
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoalBudget {
    max_iterations: usize,
    max_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Goal {
    id: String,
    objective: String,
    epic: String,
    acceptance: Vec<String>,
    status: String,
    budget: GoalBudget,
    iterations: usize,
    tasks_done: usize,
    failures: Vec<String>,
    #[serde(default)]
    resume_count: usize,
    started_at: String,
    updated_at: String,
}

fn goal_dir(ws: &Workspace) -> PathBuf {
    ws.root.join(".c2h/goals")
}

fn goal_salvar(ws: &Workspace, goal: &Goal) -> Result<(), String> {
    let dir = goal_dir(ws);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", goal.id));
    let raw = serde_json::to_string_pretty(goal).map_err(|e| e.to_string())?;
    escrever_atomico(&path, &raw)
}

fn goal_evento(ws: &Workspace, goal_id: &str, evento: serde_json::Value) -> Result<(), String> {
    let dir = goal_dir(ws);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{goal_id}.events.jsonl"));
    let mut linha = evento.to_string();
    linha.push('\n');
    use std::io::Write as _;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(linha.as_bytes()).map_err(|e| e.to_string())
}

fn goal_mais_recente(ws: &Workspace) -> Option<Goal> {
    let dir = goal_dir(ws);
    let mut entradas: Vec<_> = fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).collect();
    entradas.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entradas
        .into_iter()
        .rev()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|raw| serde_json::from_str::<Goal>(raw.trim_start_matches('\u{feff}')).ok())
        .next()
}

fn bd_rodar(ws: &Workspace, args: &[String]) -> Result<String, String> {
    let out = std::process::Command::new("bd")
        .args(args)
        .current_dir(&ws.root)
        .output()
        .map_err(|e| format!("executar bd: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "bd {:?} falhou: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(800)
                .collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn bd_json(ws: &Workspace, args: &[String]) -> Result<serde_json::Value, String> {
    let raw = bd_rodar(ws, args)?;
    serde_json::from_str(&raw).map_err(|e| format!("bd json invalido: {e}"))
}

fn verificar_goal_task(ws: &Workspace, acceptance: &str) -> Result<String, String> {
    let mut checks = 0usize;
    let mut provas = Vec::new();
    for linha in acceptance.lines() {
        let l = linha.trim();
        if let Some(nome) = l
            .strip_prefix("test:")
            .or_else(|| l.strip_prefix("run_test:"))
        {
            checks += 1;
            let nome = nome.trim();
            let saida = rodar_comando_configurado(ws, nome, "goal_verify")?;
            if !saida.contains("exit=0") {
                return Err(format!("verificacao test:{nome} falhou\n{saida}"));
            }
            provas.push(format!("test:{nome}=ok"));
        } else if let Some(path) = l.strip_prefix("file:") {
            checks += 1;
            let alvo = ws.resolver(path.trim(), true)?;
            if !alvo.exists() {
                return Err(format!(
                    "verificacao file:{} falhou: arquivo ausente",
                    path.trim()
                ));
            }
            provas.push(format!("file:{}=ok", path.trim()));
        }
    }
    if checks == 0 {
        return Err(
            "sem verificador explicito (use test:<nome> ou file:<path> no aceite)".to_string(),
        );
    }
    Ok(provas.join(", "))
}

fn git_status_progresso(ws: &Workspace) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["status", "--short", "--untracked-files=all"])
        .current_dir(&ws.root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let texto = String::from_utf8_lossy(&out.stdout);
    let mut linhas: Vec<&str> = texto
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with(".beads/")
                || t.starts_with(".c2h/")
                || t.starts_with("tmp/")
                || t.contains(" .beads/")
                || t.contains(" .c2h/")
                || t.contains(" tmp/"))
        })
        .collect();
    linhas.sort_unstable();
    Some(linhas.join("\n"))
}

fn cmd_goal_run(ws: &Workspace, epic: &str, auto: bool, system_base: &str) -> Result<(), String> {
    if !GOAL_RUNNER_ENABLED.load(Ordering::Relaxed) {
        return Err(
            "goal runner desabilitado: bd ausente no PATH ou C2H_GOAL_RUNNER=0".to_string(),
        );
    }
    let epic_json = bd_json(
        ws,
        &["show".to_string(), epic.to_string(), "--json".to_string()],
    )?;
    let issue = epic_json
        .as_array()
        .and_then(|a| a.first())
        .or_else(|| epic_json.as_object().map(|_| &epic_json))
        .ok_or_else(|| format!("epic nao encontrado: {epic}"))?;
    let objective = issue
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(epic)
        .to_string();
    let acceptance = issue
        .get("acceptance_criteria")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>();
    let goal_id = format!("goal-{}", agora_iso().replace([':', '-', '.'], ""));
    let goal = Goal {
        id: goal_id.clone(),
        objective: objective.clone(),
        epic: epic.to_string(),
        acceptance,
        status: "active".to_string(),
        budget: GoalBudget {
            max_iterations: env::var("C2H_GOAL_MAX_ITERATIONS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(5)
                .clamp(1, 50),
            max_minutes: env::var("C2H_GOAL_MAX_MINUTES")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(15)
                .clamp(1, 600),
        },
        iterations: 0,
        tasks_done: 0,
        failures: Vec::new(),
        resume_count: 0,
        started_at: agora_iso(),
        updated_at: agora_iso(),
    };
    goal_salvar(ws, &goal)?;
    goal_evento(
        ws,
        &goal.id,
        serde_json::json!({"event":"goal_started","epic":epic,"objective":objective}),
    )?;
    println!("goal: {} (epic {epic})", goal.id);
    println!("objetivo: {objective}");
    println!(
        "budget: {} iteracoes / {} min",
        goal.budget.max_iterations, goal.budget.max_minutes
    );
    executar_goal_loop(ws, goal, auto, system_base)
}

fn cmd_goal_resume(
    ws: &Workspace,
    goal_id: Option<&str>,
    auto: bool,
    system_base: &str,
) -> Result<(), String> {
    let mut goal = match goal_id {
        Some(id) => {
            let path = goal_dir(ws).join(format!("{id}.json"));
            let raw = fs::read_to_string(&path).map_err(|e| format!("goal {id}: {e}"))?;
            serde_json::from_str::<Goal>(raw.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("goal {id} invalido: {e}"))?
        }
        None => goal_mais_recente(ws).ok_or_else(|| "nenhum goal em .c2h/goals".to_string())?,
    };
    if goal.status == "done" {
        println!("goal {} ja esta done", goal.id);
        return Ok(());
    }
    goal.status = "active".to_string();
    goal.iterations = 0;
    goal.resume_count += 1;
    goal.updated_at = agora_iso();
    goal_salvar(ws, &goal)?;
    goal_evento(
        ws,
        &goal.id,
        serde_json::json!({"event":"goal_resumed","resume_count":goal.resume_count}),
    )?;
    let abertas = bd_json(
        ws,
        &[
            "list".to_string(),
            "--parent".to_string(),
            goal.epic.clone(),
            "--status".to_string(),
            "in_progress".to_string(),
            "--json".to_string(),
        ],
    )?;
    let reabertas = abertas
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("id").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for id in &reabertas {
        let _ = bd_rodar(
            ws,
            &[
                "update".to_string(),
                id.clone(),
                "--status".to_string(),
                "open".to_string(),
            ],
        );
        goal_evento(
            ws,
            &goal.id,
            serde_json::json!({"event":"task_reopened","task":id}),
        )?;
        println!("reaberta: {id}");
    }
    println!(
        "retomando goal {} ({} tarefas reabertas, tentativa {})",
        goal.id,
        reabertas.len(),
        goal.resume_count
    );
    executar_goal_loop(ws, goal, auto, system_base)
}

fn executar_goal_loop(
    ws: &Workspace,
    mut goal: Goal,
    auto: bool,
    system_base: &str,
) -> Result<(), String> {
    let epic = goal.epic.clone();
    let max_iterations = goal.budget.max_iterations;
    let max_minutes = goal.budget.max_minutes;
    let inicio = Instant::now();
    let mut ultima_falha: Option<String> = None;
    let mut falhas_seguidas = 0usize;
    while goal.iterations < max_iterations {
        if inicio.elapsed() > Duration::from_secs(max_minutes * 60) {
            goal.status = "stopped_budget_time".to_string();
            break;
        }
        let lista = bd_json(
            ws,
            &[
                "list".to_string(),
                "--parent".to_string(),
                epic.to_string(),
                "--status".to_string(),
                "open".to_string(),
                "--ready".to_string(),
                "--json".to_string(),
            ],
        )?;
        let tarefas = lista.as_array().cloned().unwrap_or_default();
        if tarefas.is_empty() {
            let abertas = bd_json(
                ws,
                &[
                    "list".to_string(),
                    "--parent".to_string(),
                    epic.to_string(),
                    "--status".to_string(),
                    "open,in_progress".to_string(),
                    "--json".to_string(),
                ],
            )?;
            let restantes = abertas.as_array().map(|a| a.len()).unwrap_or(0);
            if restantes == 0 {
                goal.status = if goal.tasks_done > 0 {
                    "done".to_string()
                } else {
                    "blocked".to_string()
                };
            } else {
                goal.status = "blocked_sem_prontas".to_string();
            }
            break;
        }
        goal.iterations += 1;
        let tarefa = &tarefas[0];
        let task_id = tarefa
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let titulo = tarefa
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let descricao = tarefa
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let aceite = tarefa
            .get("acceptance_criteria")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        println!(
            "\n[{}/{}] {} — {}",
            goal.iterations, goal.budget.max_iterations, task_id, titulo
        );
        goal_evento(
            ws,
            &goal.id,
            serde_json::json!({"event":"task_start","task":task_id,"title":titulo}),
        )?;
        if let Err(e) = bd_rodar(
            ws,
            &["update".to_string(), task_id.clone(), "--claim".to_string()],
        ) {
            goal.failures.push(format!("{task_id}: claim falhou: {e}"));
            goal.status = "blocked_claim".to_string();
            break;
        }
        let status_antes = git_status_progresso(ws);
        let tentativa = goal
            .failures
            .iter()
            .filter(|f| f.starts_with(&format!("{task_id}:")))
            .count()
            + 1;
        let retry_info = goal
            .failures
            .iter()
            .rev()
            .find(|f| f.starts_with(&format!("{task_id}:")))
            .map(|f| format!("\nTENTATIVA ANTERIOR FALHOU:\n{f}\nNao repita a mesma abordagem; corrija a causa.\n"))
            .unwrap_or_default();
        let prompt = format!(
            "Execute SOMENTE a tarefa abaixo. Nao pegue outras tarefas.\n\nTAREFA: {task_id}\nTENTATIVA: {tentativa}\nTITULO: {titulo}\nDESCRICAO:\n{descricao}\nACEITE:\n{aceite}\n{retry_info}\nAo terminar, responda com o que mudou e a evidencia. O harness vai verificar o aceite; nao declare concluido sem evidencia."
        );
        let contexto = compilar_contexto(ws, &titulo, None);
        let system = format!("{system_base}\n\n{contexto}");
        let mut hist = vec![serde_json::json!({"role":"user","content":prompt})];
        let execucao = executar_agente(&system, &mut hist, ws, auto);
        let resultado = match execucao {
            Ok(()) => {
                let prova = verificar_goal_task(ws, &aceite);
                if prova.is_ok()
                    && env::var("C2H_GOAL_REQUIRE_DIFF")
                        .map(|v| v != "0" && v != "false")
                        .unwrap_or(true)
                {
                    let status_depois = git_status_progresso(ws);
                    if let (Some(antes), Some(depois)) = (&status_antes, &status_depois) {
                        if antes == depois {
                            Err(format!(
                                "sem progresso: git status nao mudou (antes/depois iguais). Use C2H_GOAL_REQUIRE_DIFF=0 para permitir."
                            ))
                        } else {
                            prova
                        }
                    } else {
                        prova
                    }
                } else {
                    prova
                }
            }
            Err(e) => Err(format!("agente falhou: {e}")),
        };
        match resultado {
            Ok(prova) => {
                let _ = bd_rodar(ws, &["close".to_string(), task_id.clone()]);
                goal.tasks_done += 1;
                ultima_falha = None;
                falhas_seguidas = 0;
                println!("  verificacao: {prova}");
                goal_evento(
                    ws,
                    &goal.id,
                    serde_json::json!({"event":"task_done","task":task_id,"proof":prova}),
                )?;
            }
            Err(e) => {
                let _ = bd_rodar(
                    ws,
                    &[
                        "update".to_string(),
                        task_id.clone(),
                        "--status".to_string(),
                        "open".to_string(),
                    ],
                );
                goal.failures.push(format!("{task_id}: {e}"));
                if e.contains("sem progresso") {
                    goal_evento(
                        ws,
                        &goal.id,
                        serde_json::json!({"event":"task_no_progress","task":task_id}),
                    )?;
                }
                println!("  falhou: {e}");
                goal_evento(
                    ws,
                    &goal.id,
                    serde_json::json!({"event":"task_failed","task":task_id,"error":e}),
                )?;
                if ultima_falha.as_deref() == Some(task_id.as_str()) {
                    falhas_seguidas += 1;
                } else {
                    ultima_falha = Some(task_id.clone());
                    falhas_seguidas = 1;
                }
                if falhas_seguidas >= 2 {
                    goal.status = "blocked_repeated_failure".to_string();
                    break;
                }
            }
        }
        goal.updated_at = agora_iso();
        goal_salvar(ws, &goal)?;
    }
    if goal.status == "active" {
        goal.status = if goal.iterations >= max_iterations {
            "stopped_budget_iterations".to_string()
        } else {
            "stopped".to_string()
        };
    }
    goal.updated_at = agora_iso();
    goal_salvar(ws, &goal)?;
    goal_evento(
        ws,
        &goal.id,
        serde_json::json!({"event":"goal_stop","status":goal.status,"iterations":goal.iterations,"tasks_done":goal.tasks_done}),
    )?;
    println!("\ngoal {}: {}", goal.id, goal.status);
    println!(
        "iteracoes: {} | tarefas concluidas: {} | falhas: {}",
        goal.iterations,
        goal.tasks_done,
        goal.failures.len()
    );
    Ok(())
}

fn cmd_goal_status(ws: &Workspace) {
    match goal_mais_recente(ws) {
        Some(g) => {
            println!("goal: {}", g.id);
            println!("objetivo: {}", g.objective);
            println!("epic: {}", g.epic);
            println!("status: {}", g.status);
            println!("iteracoes: {}/{}", g.iterations, g.budget.max_iterations);
            println!("tarefas concluidas: {}", g.tasks_done);
            if !g.failures.is_empty() {
                println!("ultima falha: {}", g.failures.last().unwrap());
            }
        }
        None => println!("nenhum goal em .c2h/goals"),
    }
}

fn goals_lidos(ws: &Workspace) -> Vec<Goal> {
    let Ok(entradas) = fs::read_dir(goal_dir(ws)) else {
        return Vec::new();
    };
    let mut arquivos: Vec<_> = entradas.filter_map(|e| e.ok()).collect();
    arquivos.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    arquivos
        .into_iter()
        .rev()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|raw| serde_json::from_str::<Goal>(raw.trim_start_matches('\u{feff}')).ok())
        .collect()
}

fn goal_ativo(ws: &Workspace) -> Option<Goal> {
    let mut goals = goals_lidos(ws);
    goals.sort_by_key(|g| g.updated_at.clone());
    if let Some(g) = goals.iter().rev().find(|g| g.status != "done") {
        return Some(g.clone());
    }
    goals.pop()
}

fn goal_tarefas_bd(ws: &Workspace, epic: &str) -> Result<Vec<serde_json::Value>, String> {
    let lista = bd_json(
        ws,
        &[
            "list".to_string(),
            "--parent".to_string(),
            epic.to_string(),
            "--status".to_string(),
            "open,in_progress,blocked,deferred,closed".to_string(),
            "--json".to_string(),
        ],
    )?;
    Ok(lista.as_array().cloned().unwrap_or_default())
}

fn goal_ready_ids(ws: &Workspace, epic: &str) -> std::collections::BTreeSet<String> {
    bd_json(
        ws,
        &[
            "list".to_string(),
            "--parent".to_string(),
            epic.to_string(),
            "--status".to_string(),
            "open".to_string(),
            "--ready".to_string(),
            "--json".to_string(),
        ],
    )
    .ok()
    .and_then(|v| v.as_array().cloned())
    .unwrap_or_default()
    .iter()
    .filter_map(|t| t.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
    .collect()
}

fn goal_tarefa_atual(
    _ws: &Workspace,
    goal: &Goal,
    tarefas: &[serde_json::Value],
    ready: &std::collections::BTreeSet<String>,
) -> Option<serde_json::Value> {
    if let Some(t) = tarefas
        .iter()
        .find(|t| t.get("status").and_then(|v| v.as_str()) == Some("in_progress"))
    {
        return Some(t.clone());
    }
    tarefas
        .iter()
        .find(|t| {
            t.get("status").and_then(|v| v.as_str()) == Some("open")
                && t.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| ready.contains(id))
                    .unwrap_or(false)
        })
        .cloned()
        .or_else(|| {
            let _ = goal;
            None
        })
}

fn goal_eventos_lidos(ws: &Workspace, goal_id: &str, limit: usize) -> Vec<serde_json::Value> {
    let path = goal_dir(ws).join(format!("{goal_id}.events.jsonl"));
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut eventos: Vec<_> = raw
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .collect();
    if eventos.len() > limit {
        eventos = eventos.split_off(eventos.len() - limit);
    }
    eventos
}

fn goal_resumo_valor(ws: &Workspace, goal: &Goal) -> Result<serde_json::Value, String> {
    let tarefas = goal_tarefas_bd(ws, &goal.epic)?;
    let ready = goal_ready_ids(ws, &goal.epic);
    let atual = goal_tarefa_atual(ws, goal, &tarefas, &ready);
    let restantes = tarefas
        .iter()
        .filter(|t| t.get("status").and_then(|v| v.as_str()) != Some("closed"))
        .count();
    let valor = serde_json::json!({
        "id": goal.id,
        "objective": goal.objective,
        "epic": goal.epic,
        "status": goal.status,
        "iterations": goal.iterations,
        "max_iterations": goal.budget.max_iterations,
        "tasks_done": goal.tasks_done,
        "tasks_remaining": restantes,
        "resume_count": goal.resume_count,
        "current_task": atual.as_ref().map(|t| serde_json::json!({
            "id": t.get("id"),
            "title": t.get("title"),
            "status": t.get("status"),
            "acceptance": t.get("acceptance_criteria"),
        })),
        "last_error": goal.failures.last(),
        "updated_at": goal.updated_at,
    });
    Ok(valor)
}

fn ferramenta_goal_status(ws: &Workspace) -> Result<String, String> {
    let Some(goal) = goal_ativo(ws) else {
        return Ok(serde_json::json!({"goal": null, "hint": "use /goal run <epic>"}).to_string());
    };
    serde_json::to_string_pretty(&goal_resumo_valor(ws, &goal)?).map_err(|e| e.to_string())
}

fn ferramenta_goal_tasks(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let Some(goal) = goal_ativo(ws) else {
        return Ok(serde_json::json!({"goal": null, "tasks": []}).to_string());
    };
    let filtro = args
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open");
    let so_prontas = args.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
    let tarefas = goal_tarefas_bd(ws, &goal.epic)?;
    let ready = goal_ready_ids(ws, &goal.epic);
    let saida: Vec<_> = tarefas
        .into_iter()
        .filter(|t| {
            let status = t.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match filtro {
                "all" => true,
                "closed" => status == "closed",
                _ => status != "closed",
            }
        })
        .filter(|t| {
            if so_prontas {
                t.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| ready.contains(id))
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .map(|t| {
            let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
            serde_json::json!({
                "id": id,
                "title": t.get("title"),
                "status": t.get("status"),
                "priority": t.get("priority"),
                "type": t.get("issue_type"),
                "acceptance": t.get("acceptance_criteria"),
                "ready": ready.contains(id),
            })
        })
        .collect();
    Ok(serde_json::json!({"goal": goal.id, "epic": goal.epic, "tasks": saida}).to_string())
}

fn ferramenta_goal_next(ws: &Workspace) -> Result<String, String> {
    let Some(goal) = goal_ativo(ws) else {
        return Ok(serde_json::json!({"goal": null, "next": null}).to_string());
    };
    let tarefas = goal_tarefas_bd(ws, &goal.epic)?;
    let ready = goal_ready_ids(ws, &goal.epic);
    let proxima = tarefas.iter().find(|t| {
        t.get("id")
            .and_then(|v| v.as_str())
            .map(|id| ready.contains(id))
            .unwrap_or(false)
            && t.get("status").and_then(|v| v.as_str()) == Some("open")
    });
    Ok(serde_json::json!({
        "goal": goal.id,
        "next": proxima.map(|t| serde_json::json!({
            "id": t.get("id"),
            "title": t.get("title"),
            "acceptance": t.get("acceptance_criteria"),
            "description": t.get("description"),
        })),
    })
    .to_string())
}

fn ferramenta_goal_context(ws: &Workspace) -> Result<String, String> {
    let Some(goal) = goal_ativo(ws) else {
        return Ok(serde_json::json!({"goal": null, "context": null}).to_string());
    };
    let mut resumo = goal_resumo_valor(ws, &goal)?;
    if let Some(obj) = resumo.as_object_mut() {
        obj.insert(
            "recent_events".to_string(),
            serde_json::json!(goal_eventos_lidos(ws, &goal.id, 10)),
        );
    }
    serde_json::to_string_pretty(&resumo).map_err(|e| e.to_string())
}

fn ferramenta_goal_events(ws: &Workspace, args: &serde_json::Value) -> Result<String, String> {
    let Some(goal) = goal_ativo(ws) else {
        return Ok(serde_json::json!({"goal": null, "events": []}).to_string());
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .clamp(1, 100) as usize;
    Ok(
        serde_json::json!({"goal": goal.id, "events": goal_eventos_lidos(ws, &goal.id, limit)})
            .to_string(),
    )
}

fn cmd_chat(yolo: bool) -> Result<(), String> {
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
    let bd_ok = std::process::Command::new("bd")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    BD_DISPONIVEL.store(bd_ok, Ordering::Relaxed);
    let goal_ligado = bd_ok
        && env::var("C2H_GOAL_RUNNER")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(true);
    GOAL_RUNNER_ENABLED.store(goal_ligado, Ordering::Relaxed);
    if !bd_ok {
        println!("aviso: bd (beads) nao encontrado no PATH.");
        println!("       goal runner/plano fica DESABILITADO; instale beads e adicione ao PATH.");
    } else if !goal_ligado {
        println!("aviso: goal runner desabilitado por C2H_GOAL_RUNNER=0.");
    }
    println!("digite /help para comandos; /sair para encerrar.\n");

    let system_base = "Voce e o c2h, um agente de codigo em terminal. Responda de forma curta, pratica e em portugues. Nao escreva preambulo. Se a tarefa for criar arquivo, chame write_file/new_component imediatamente. So responda em texto depois do resultado da ferramenta. Use as ferramentas para listar, ler, buscar e escrever arquivos. Se nao souber o caminho exato, chame list_dir antes de read_file; nunca invente nome de arquivo. Leia cada arquivo uma vez e use start/end; nao releia arquivo inalterado. Se o pedido for um plano, faca no maximo 3 leituras e responda o plano; nao explore o repo inteiro. Quando o usuario pedir para criar algo, chame write_file. Para arquivo existente, chame replace_file; nunca use remove_file para editar. O W5H2 deve ser teleologico: what e a responsabilidade, why e o proposito/causa tecnica, how e o mecanismo. Nao invente arquivos ou APIs. NUNCA use shell para ler codigo (type/cat/findstr/grep/Get-Content) nem para redirecionar (>,>>) para arquivo de codigo; use read_file/find_text/write_file/replace_file/edit_file. Shell e apenas para executar comandos.";
    let mut historico: Vec<serde_json::Value> = Vec::new();
    let mut arquivo: Option<String> = None;
    let mut auto = yolo
        || env::var("C2H_YOLO")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
    AUTO_CONFIRM.store(auto, Ordering::Relaxed);
    if auto {
        println!("YOLO ATIVO: escrita/rm/mv sem confirmacao.");
    }
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
                "\nComandos:\n  /pwd               diretorio atual\n  /ls [dir]          lista arquivos\n  /cd <dir>          muda de diretorio\n  /tree [n]          arvore do projeto\n  /read <f> [a:b]    le arquivo\n  /find <texto>      busca no projeto\n  /imports <f>       lista imports/links\n  /write <f> \"txt\"  cria arquivo com IA\n  /edit <f> \"txt\"   edita por blocos\n  /new component <n> scaffold HTML/CSS/JS\n  /rm <f>            remove com backup\n  /mv <a> <b>        move com backup\n  /preview <f>       abre HTML no navegador\n  /annotate <f>      descobre blocos com IA se vazio\n  /auto ou /yolo      ativa/desativa confirmacao de escrita
  /goal run <epic>    executa tarefas prontas do epic via bd
  /goal resume [id]   retoma goal; reabre in_progress orfas
  /goal status        mostra o goal mais recente
  /shell [on|off]     liga/desliga shell livre (padrao off)\n  /tools              lista ferramentas do agente\n  /file <f>          anexa contexto\n  /drop              remove arquivo ativo\n  /clear             limpa historico\n  /status            estado da sessao\n  /doctor            diagnostico do harness\n  /logs [n]          ultimos eventos da timeline (/log [n] idem)\n  /sair              encerra\n"
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
        if entrada == "/doctor" {
            if let Err(erro) = cmd_doctor() {
                println!("erro: {erro}");
            }
            continue;
        }
        if entrada == "/log" || entrada == "/logs" {
            if let Err(erro) = cmd_logs(40) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/log ") {
            let limite = resto.trim().parse::<usize>().unwrap_or(40);
            if let Err(erro) = cmd_logs(limite) {
                println!("erro: {erro}");
            }
            continue;
        }
        if let Some(resto) = entrada.strip_prefix("/logs ") {
            let limite = resto.trim().parse::<usize>().unwrap_or(40);
            if let Err(erro) = cmd_logs(limite) {
                println!("erro: {erro}");
            }
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
        if entrada == "/goal" || entrada.starts_with("/goal ") {
            let resto = entrada.trim_start_matches("/goal").trim();
            if resto == "status" || resto.is_empty() {
                cmd_goal_status(&ws);
                continue;
            }
            if let Some(id) = resto.strip_prefix("resume") {
                if let Err(e) = cmd_goal_resume(
                    &ws,
                    Some(id.trim()).filter(|s| !s.is_empty()),
                    auto,
                    system_base,
                ) {
                    println!("erro goal: {e}");
                }
                continue;
            }
            if let Some(epic) = resto.strip_prefix("run ") {
                if let Err(e) = cmd_goal_run(&ws, epic.trim(), auto, system_base) {
                    println!("erro goal: {e}");
                }
                continue;
            }
            println!("uso: /goal run <epic-bd> | /goal resume [id] | /goal status");
            continue;
        }
        if entrada == "/status" {
            println!("projeto: {}", ws.root.display());
            let cwd_exibicao = if ws.cwd.as_os_str().is_empty() {
                "/".to_string()
            } else {
                format!("/{}", ws.cwd.to_string_lossy().replace('\\', "/"))
            };
            println!("cwd: {cwd_exibicao}");
            println!("modelo: {} - provider: {}", model(), base_url());
            println!("arquivo ativo: {}", arquivo.as_deref().unwrap_or("nenhum"));
            println!("mensagens: {}", historico.len());
            println!(
                "bd: {}",
                if BD_DISPONIVEL.load(Ordering::Relaxed) {
                    "disponivel"
                } else {
                    "ausente (goal runner off)"
                }
            );
            println!(
                "goal runner: {}",
                if GOAL_RUNNER_ENABLED.load(Ordering::Relaxed) {
                    "on"
                } else {
                    "off"
                }
            );
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
        if entrada == "/auto" || entrada == "/yolo" {
            auto = !auto;
            AUTO_CONFIRM.store(auto, Ordering::Relaxed);
            println!("auto={auto} (escrita sem confirmacao)");
            continue;
        }
        if entrada == "/shell" || entrada.starts_with("/shell ") {
            let arg = entrada.trim_start_matches("/shell").trim();
            match arg {
                "on" => {
                    SHELL_ENABLED.store(true, Ordering::Relaxed);
                    println!("shell ON (livre; use com cuidado)");
                }
                "off" => {
                    SHELL_ENABLED.store(false, Ordering::Relaxed);
                    println!("shell OFF (use run_test/run_script/git/bd)");
                }
                _ => println!(
                    "shell={} (use /shell on|off; padrao off)",
                    if SHELL_ENABLED.load(Ordering::Relaxed) {
                        "on"
                    } else {
                        "off"
                    }
                ),
            }
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
static AUTO_CONFIRM: AtomicBool = AtomicBool::new(false);
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
    if !path_anotavel(&rel_file(file)) {
        return false;
    }
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

fn caminho_cache_anotacao(conteudo: &str) -> PathBuf {
    let digest = hash(conteudo).replace("sha256:", "");
    PathBuf::from(".c2h/cache/annotations").join(format!("{digest}.json"))
}

fn anotar_blocos_ia(file: &Path) -> Result<bool, String> {
    if !path_anotavel(&rel_file(file)) {
        log_evento(
            "annotation_skipped_scope",
            serde_json::json!({"file": rel_file(file)}),
        );
        println!(
            "annotation pass ignorado pela politica de escopo: {}",
            rel_file(file)
        );
        return Ok(false);
    }
    let conteudo = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let linhas: Vec<&str> = conteudo.lines().collect();
    let cache_path = caminho_cache_anotacao(&conteudo);
    if let Ok(texto) = fs::read_to_string(&cache_path) {
        if let Ok(blocos) = serde_json::from_str::<Vec<SidecarBlock>>(&texto) {
            if !blocos.is_empty() {
                let sidecar = Sidecar {
                    schema_version: SCHEMA_VERSION,
                    contract_version: CONTRACT_VERSION,
                    file: rel_file(file),
                    status: "annotated".to_string(),
                    source_hash: hash(&conteudo),
                    generated_by: "c2h-cache".to_string(),
                    blocks: blocos,
                };
                escrever_sidecar(file, &sidecar)?;
                log_evento(
                    "annotation_cache_hit",
                    serde_json::json!({"file": rel_file(file), "blocks": sidecar.blocks.len()}),
                );
                println!("annotation pass: cache reaproveitado");
                return Ok(true);
            }
        }
    }
    let system = "Voce identifica blocos de responsabilidade em um arquivo. Responda APENAS JSON: {\"blocks\":[{\"block_id\":\"id-unico\",\"kind\":\"section|function|class|block\",\"start_line\":1,\"end_line\":10,\"what\":\"...\",\"why\":\"causa->efeito tecnico\",\"when\":\"quando e alcancado\",\"where\":\"contexto\",\"who\":\"quem usa\",\"how\":\"como funciona\",\"how_much\":\"limites/custo\",\"risk\":\"high|medium|low\",\"confidence\":\"high|medium|low\",\"verified_by\":\"none|code|test|human\"}]}. Use 1 a 12 blocos, sem sobreposicao. Priorize blocos centrais de I/O, formato, parsing, persistencia, encode/decode e validacao; marque risk high neles. O what e a responsabilidade; o why e teleologico: por que o bloco existe, que problema evita e que objetivo garante. O how e o mecanismo. Nunca use \"solicitado pelo agente\". Nao escreva timestamp.";
    let comentarios = extrair_comentarios(&conteudo, 60);
    let mut user = format!(
        "ARQUIVO: {}\nTotal de linhas: {}\nCOMENTARIOS/DOCSTRINGS:\n{}\nConteudo:\n{}",
        rel_file(file),
        linhas.len(),
        comentarios,
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
                        origin: "annotation".to_string(),
                        risk: item
                            .get("risk")
                            .and_then(|v| v.as_str())
                            .unwrap_or("medium")
                            .to_string(),
                        confidence: item
                            .get("confidence")
                            .and_then(|v| v.as_str())
                            .unwrap_or("medium")
                            .to_string(),
                        verified_by: item
                            .get("verified_by")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none")
                            .to_string(),
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
                            goal: item
                                .get("goal")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            business_logic: item
                                .get("business_logic")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            when: item
                                .get("when")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            where_: item
                                .get("where")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            who: item
                                .get("who")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            how: item
                                .get("how")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            how_much: item
                                .get("how_much")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
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
                if let Some(pai) = cache_path.parent() {
                    let _ = fs::create_dir_all(pai);
                }
                if let Ok(texto_cache) = serde_json::to_string_pretty(&sidecar.blocks) {
                    let _ = fs::write(&cache_path, texto_cache);
                }
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
                origin: "structural".to_string(),
                risk: risco_heuristico(&bloco.name, &bloco.kind),
                confidence: "medium".to_string(),
                verified_by: "none".to_string(),
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
                origin: "structural".to_string(),
                risk: risco_heuristico(&bloco.name, &bloco.kind),
                confidence: "medium".to_string(),
                verified_by: "none".to_string(),
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
        status: if !path_anotavel(&rel_file(file)) {
            "disposable"
        } else if novos.is_empty() || novos.iter().any(|b| !why_tecnico(&b.w5h2.why)) {
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
            goal: texto("goal"),
            business_logic: texto("business_logic"),
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
    fn le_commands_toml() {
        let dir = env::temp_dir().join(format!("c2h-toml-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("commands.toml");
        fs::write(
            &path,
            "[commands]\ntest = \"echo oi\"\ntest_all = \"a && b\"\n",
        )
        .unwrap();
        assert_eq!(comando_toml(&path, "test").as_deref(), Some("echo oi"));
        assert_eq!(comando_toml(&path, "test_all").as_deref(), Some("a && b"));
        assert_eq!(comando_toml(&path, "nada"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn classifica_shell() {
        assert_eq!(classe_shell("type a.rs"), "read_code");
        assert_eq!(classe_shell("echo x > a.rs"), "write_code");
        assert_eq!(classe_shell("node tools/smoke-live.js"), "test_build");
        assert_eq!(classe_shell("git status"), "unknown");
    }

    #[test]
    fn detecta_leitura_de_codigo_por_shell() {
        let caso = detectar_bypass_shell("findstr /n /c:\"x\" moebius/app/server.js");
        assert_eq!(caso.map(|(k, _)| k), Some("read_code"));
    }

    #[test]
    fn nao_detecta_log_como_codigo() {
        assert!(detectar_bypass_shell("type moebius-detached.log").is_none());
    }

    #[test]
    fn detecta_redirecionamento_em_codigo() {
        let caso = detectar_bypass_shell("echo x > src/index.js");
        assert_eq!(caso.map(|(k, _)| k), Some("write_code"));
    }

    #[test]
    fn nao_confunde_log_com_redirecionamento_de_codigo() {
        assert!(detectar_bypass_shell("node tools/smoke.js > tmp/reg.log").is_none());
    }

    #[test]
    fn why_generico_e_rejeitado() {
        assert!(!why_tecnico("solicitado pelo agente"));
        assert!(!why_tecnico("necessario"));
        assert!(why_tecnico(
            "read_file e assincrona porque devolve dados comprimidos"
        ));
    }

    #[test]
    fn extrai_comentarios_do_codigo() {
        let texto = "// why: assincrono para descomprimir\nfn a() {}\n# comentario\n";
        let c = extrair_comentarios(texto, 10);
        assert!(c.contains("assincrono"));
        assert!(c.contains("comentario"));
    }

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
    fn exige_evidencia_em_toda_mudanca() {
        let _cwd = CWD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "c2h-evid-{}-{}",
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
                evidence: vec![],
            }],
        };
        let violacoes = validar_resposta(Path::new("."), &env);
        assert!(violacoes.iter().any(|v| v.code == "E-EVID-00"));
        std::env::set_current_dir(anterior).unwrap();
        std::fs::remove_dir_all(dir).ok();
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
        None | Some(Command::Chat) => cmd_chat(cli.yolo),
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
