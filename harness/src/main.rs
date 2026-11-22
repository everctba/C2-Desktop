//! C2 Harness (c2h): CLI minimo para contexto, blocos e anotacoes.
//!
//! O harness e dono de block_id, hashes e timestamps. A IA e usada apenas para
//! gerar conteudo; nada e aplicado sem validacao.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

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
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
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
            let caminho = ev.split(':').next().unwrap_or(ev);
            if !raiz.join(caminho).exists() {
                erro(
                    "E-EVID-01",
                    Some(&change.block_id),
                    "evidencia nao existe",
                    "use arquivo:linha existente",
                );
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

fn chamar_provider(system: &str, user: &str) -> Result<String, String> {
    log_evento(
        "provider_request",
        serde_json::json!({"model": model(), "system_chars": system.len(), "user_chars": user.len()}),
    );
    if let Ok(mock) = env::var("C2H_MOCK_RESPONSE") {
        log_evento(
            "provider_mock",
            serde_json::json!({"response_chars": mock.len()}),
        );
        return Ok(mock);
    }
    let key = api_key().ok_or_else(|| "C2H_API_KEY/DEEPSEEK_API_KEY ausente".to_string())?;
    let body = serde_json::json!({
        "model": model(),
        "messages": [
            {"role":"system","content":system},
            {"role":"user","content":user}
        ],
        "temperature": 0.1,
        "stream": false
    });
    let url = format!("{}/chat/completions", base_url().trim_end_matches('/'));
    let resposta = ureq::post(&url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120))
        .send_json(body)
        .map_err(|e| format!("provider: {e}"))?;
    let json: serde_json::Value = resposta.into_json().map_err(|e| format!("json: {e}"))?;
    let content = json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "resposta sem choices[0].message.content".to_string())?;
    log_evento(
        "provider_response",
        serde_json::json!({"chars": content.len()}),
    );
    Ok(content)
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
                let b = blocos.iter().find(|b| b.id == c.block_id)?;
                Some((b.start_line, b.end_line, *c))
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
            cmd_annotate(Path::new(arquivo), false)?;
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

fn nome_comando(command: &Command) -> &'static str {
    match command {
        Command::Doctor => "doctor",
        Command::Blocks { .. } => "blocks",
        Command::Annotate { .. } => "annotate",
        Command::Validate { .. } => "validate",
        Command::Status => "status",
        Command::Context { .. } => "context",
        Command::Ask { .. } => "ask",
        Command::ValidateResponse { .. } => "validate_response",
        Command::Apply { .. } => "apply",
        Command::Logs { .. } => "logs",
        Command::Fuzz { .. } => "fuzz",
        Command::Config => "config",
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
                created_at: agora.clone(),
                updated_at: agora.clone(),
                semantic_hash: bloco.semantic_hash.clone(),
                w5h2: W5h2::default(),
                evidence: Vec::new(),
            }
        };
        novos.push(side);
    }
    let sidecar = Sidecar {
        schema_version: SCHEMA_VERSION,
        contract_version: CONTRACT_VERSION,
        file: rel_file(file),
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
    fn extrai_json_de_fence() {
        let env = extrair_json("```json\n{\"contract_version\":1,\"changes\":[]}\n```").unwrap();
        assert_eq!(env.contract_version, 1);
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
        serde_json::json!({"command": nome_comando(&cli.command)}),
    );
    let resultado = match &cli.command {
        Command::Doctor => cmd_doctor(),
        Command::Blocks { file } => cmd_blocks(file, cli.json),
        Command::Annotate { file, dry_run } => cmd_annotate(file, *dry_run),
        Command::Validate { file } => cmd_validate(file.as_deref()),
        Command::Status => cmd_status(),
        Command::Context { file, max_chars } => cmd_context(file, *max_chars),
        Command::Ask {
            prompt,
            system,
            file,
            apply,
            max_repairs,
        } => cmd_ask(
            prompt,
            system.as_deref(),
            file.as_deref(),
            *apply,
            *max_repairs,
        ),
        Command::ValidateResponse { response } => cmd_validate_response(response),
        Command::Apply { response, dry_run } => cmd_apply(response, *dry_run),
        Command::Logs { limit } => cmd_logs(*limit),
        Command::Fuzz { iters } => cmd_fuzz(*iters),
        Command::Config => {
            cmd_config();
            Ok(())
        }
    };
    if let Err(erro) = resultado {
        eprintln!("c2h: {erro}");
        std::process::exit(1);
    }
}
