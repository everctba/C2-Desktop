//! C2 Harness (c2h): CLI minimo para contexto, blocos e anotacoes.
//!
//! O harness e dono de block_id, hashes e timestamps. A IA e usada apenas para
//! gerar conteudo; nada e aplicado sem validacao.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
        "rs" => extrair_rust(conteudo),
        "py" => extrair_python(conteudo),
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
        if !(t.starts_with("<section") || t.contains("data-c2-block")) {
            continue;
        }
        let nome = t
            .split("data-c2-block=\"")
            .nth(1)
            .and_then(|x| x.split('"').next())
            .or_else(|| t.split("id=\"").nth(1).and_then(|x| x.split('"').next()))
            .unwrap_or("anon")
            .to_string();
        let mut prof = 0i32;
        let mut fim = i + 1;
        for (j, l) in linhas.iter().enumerate().skip(i) {
            prof += l.matches("<section").count() as i32;
            prof -= l.matches("</section>").count() as i32;
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
        println!("sidecar: {}", sidecar_path(file).display());
        println!("blocos: {}", sidecar.blocks.len());
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

fn api_key() -> Option<String> {
    env::var("C2H_API_KEY")
        .ok()
        .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
        .or_else(|| env::var("OPENCODEGO_API_KEY").ok())
        .filter(|v| !v.trim().is_empty())
}

fn base_url() -> String {
    env::var("C2H_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com/v1".to_string())
}

fn model() -> String {
    env::var("C2H_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string())
}

fn cmd_ask(prompt: &str, system: Option<&str>) -> Result<(), String> {
    let key = api_key().ok_or_else(|| "C2H_API_KEY/DEEPSEEK_API_KEY ausente".to_string())?;
    let system = system.unwrap_or(
        "Voce e um agente de codigo. Responda em JSON quando o harness pedir contrato; nao invente arquivos ou IDs.",
    );
    let body = serde_json::json!({
        "model": model(),
        "messages": [
            {"role":"system","content":system},
            {"role":"user","content":prompt}
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
        .ok_or_else(|| "resposta sem choices[0].message.content".to_string())?;
    println!("{content}");
    Ok(())
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
    let resultado = match &cli.command {
        Command::Doctor => cmd_doctor(),
        Command::Blocks { file } => cmd_blocks(file, cli.json),
        Command::Annotate { file, dry_run } => cmd_annotate(file, *dry_run),
        Command::Validate { file } => cmd_validate(file.as_deref()),
        Command::Context { file, max_chars } => cmd_context(file, *max_chars),
        Command::Ask { prompt, system } => cmd_ask(prompt, system.as_deref()),
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
