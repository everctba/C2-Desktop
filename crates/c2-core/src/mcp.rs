//! Servidor MCP minimo sobre STDIO (beads `c2-e13t1..t3`, `c2-e16t5`).
//!
//! Expoe o Core a agentes sem duplicar logica: JSON-RPC 2.0 por linha, com
//! `initialize`, `tools/list` e `tools/call`. Somente leitura, resposta limitada
//! e logs em stderr (stdout e o canal do protocolo).

use std::path::Path;

use serde_json::{json, Value};

use crate::config::Config;
use crate::error::Result;
use crate::schema::{meta_path_for, MetaFile};
use crate::search::{self, SearchOptions};
use crate::workspace::Workspace;

/// Versao do protocolo MCP suportada.
pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// Nome reportado no handshake.
pub const SERVER_NAME: &str = "c2";

fn tronco(texto: &str, limite: usize) -> String {
    if texto.chars().count() <= limite {
        return texto.to_string();
    }
    let cortado: String = texto.chars().take(limite).collect();
    format!("{cortado}… (truncado)")
}

/// Contrato das ferramentas (ordem estavel para o painel).
pub fn tools() -> Value {
    json!([
        {
            "name": "c2_grep",
            "description": "Busca semantica nos metadados do projeto. Retorna arquivo, score, resumo e trecho.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "O que procurar (por significado)" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 10, "default": 5 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "c2_read",
            "description": "Le um arquivo do projeto com numero de linha (faixa opcional).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Caminho relativo ao workspace" },
                    "start_line": { "type": "integer", "minimum": 1, "default": 1 },
                    "end_line": { "type": "integer", "minimum": 1, "default": 80 }
                },
                "required": ["file"]
            }
        },
        {
            "name": "c2_related",
            "description": "Dependencias de um arquivo pelo grafo estatico (usa / usado por).",
            "inputSchema": {
                "type": "object",
                "properties": { "file": { "type": "string" } },
                "required": ["file"]
            }
        },
        {
            "name": "c2_map",
            "description": "Arvore plana do projeto: uma linha por arquivo indexavel.",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 } }
            }
        },
        {
            "name": "c2_annotate",
            "description": "Anexa uma anotacao de agente (teleology/invariant/gotcha) ao .c2.json do arquivo. Escrita restrita a agent_notes: nao altera codigo, semantic nem links. Use somente para descobertas nao obvias pelo codigo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Caminho relativo ao workspace" },
                    "text": { "type": "string", "description": "Descoberta nao obvia (max 2000 caracteres)" },
                    "kind": { "type": "string", "enum": ["teleology", "invariant", "gotcha", "note"], "default": "note" },
                    "task": { "type": "string", "description": "Tarefa/contexto em que a anotacao nasceu (opcional)" },
                    "model": { "type": "string", "description": "Modelo do agente (opcional)" }
                },
                "required": ["file", "text"]
            }
        }
    ])
}

/// Nomes das ferramentas (para painel/testes).
pub fn tool_names() -> Vec<&'static str> {
    vec!["c2_grep", "c2_read", "c2_related", "c2_map", "c2_annotate"]
}

/// Processa uma linha JSON-RPC. Devolve `None` para notificacoes (sem `id`).
pub fn handle(workspace: &Workspace, config: &Config, linha: &str) -> Option<String> {
    let mensagem: Value = match serde_json::from_str(linha) {
        Ok(valor) => valor,
        Err(erro) => {
            return Some(erro_json(
                Value::Null,
                -32700,
                &format!("JSON invalido: {erro}"),
            ));
        }
    };

    let Some(object) = mensagem.as_object() else {
        return Some(erro_json(
            Value::Null,
            -32600,
            "mensagem deve ser um objeto",
        ));
    };
    let id = object.get("id").cloned();
    let metodo = object
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = object.get("params").cloned().unwrap_or(Value::Null);

    // Notificacao: sem `id` nao ha resposta.
    let id = id?;

    let resultado = match metodo {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let nome = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let argumentos = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match chamar_ferramenta(workspace, config, nome, &argumentos) {
                Ok(texto) => Ok(json!({
                    "content": [{ "type": "text", "text": texto }],
                    "isError": false
                })),
                Err(erro) => Ok(json!({
                    "content": [{ "type": "text", "text": format!("erro: {erro}") }],
                    "isError": true
                })),
            }
        }
        "ping" => Ok(json!({})),
        outro => Err((
            -32601,
            format!(
                "metodo nao suportado: {outro} (aqui so initialize/tools/list/tools/call/ping)"
            ),
        )),
    };

    Some(match resultado {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string(),
        Err((codigo, mensagem)) => erro_json(id, codigo, &mensagem),
    })
}

fn erro_json(id: Value, codigo: i64, mensagem: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": codigo, "message": mensagem }
    })
    .to_string()
}

fn texto_erro(mensagem: impl Into<String>) -> crate::error::C2Error {
    crate::error::C2Error::InvalidConfig(mensagem.into())
}

fn chamar_ferramenta(
    workspace: &Workspace,
    config: &Config,
    nome: &str,
    argumentos: &Value,
) -> Result<String> {
    match nome {
        "c2_grep" => {
            let consulta = argumentos
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| texto_erro("c2_grep exige 'query'"))?;
            let limite = argumentos
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 10) as usize;
            let opcoes = SearchOptions {
                limit: limite,
                include_snippets: true,
            };
            let hits = search::search(workspace, config, consulta, &opcoes)?;
            if hits.is_empty() {
                return Ok(format!("nenhum resultado para \"{consulta}\""));
            }
            let mut saida = String::new();
            for (indice, hit) in hits.iter().enumerate() {
                saida.push_str(&format!(
                    "{}. {} (score {:.2})\n   {}\n",
                    indice + 1,
                    hit.file,
                    hit.score,
                    tronco(&hit.summary, 240)
                ));
                if let Some(trecho) = &hit.snippet {
                    saida.push_str(&format!(
                        "   linhas {}-{}: {}\n",
                        trecho.line_start,
                        trecho.line_end,
                        tronco(&trecho.text.replace('\n', " ⏎ "), 300)
                    ));
                }
            }
            Ok(saida)
        }
        "c2_read" => {
            let rel = argumentos
                .get("file")
                .and_then(Value::as_str)
                .ok_or_else(|| texto_erro("c2_read exige 'file'"))?;
            let caminho = workspace
                .root()
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let seguro = workspace.ensure_inside(&caminho)?;
            let conteudo = std::fs::read_to_string(&seguro)
                .map_err(|erro| texto_erro(format!("nao foi possivel ler {rel}: {erro}")))?;
            let linhas: Vec<&str> = conteudo.lines().collect();
            let inicio = argumentos
                .get("start_line")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1) as usize;
            let fim = argumentos
                .get("end_line")
                .and_then(Value::as_u64)
                .unwrap_or((inicio + 79) as u64)
                .min((inicio + 199) as u64) as usize;
            let inicio = inicio.min(linhas.len().max(1));
            let fim = fim.min(linhas.len());
            let mut saida = format!("{rel} (linhas {inicio}-{fim} de {})\n", linhas.len());
            for numero in inicio..=fim {
                if let Some(linha) = linhas.get(numero - 1) {
                    saida.push_str(&format!("{numero:>5} | {}\n", tronco(linha, 200)));
                }
            }
            Ok(saida)
        }
        "c2_related" => {
            let rel = argumentos
                .get("file")
                .and_then(Value::as_str)
                .ok_or_else(|| texto_erro("c2_related exige 'file'"))?;
            let meta = MetaFile::read(&meta_path_for(Path::new(rel))).ok();
            let mut usa: Vec<String> = Vec::new();
            if let Some(meta) = &meta {
                usa = meta
                    .links
                    .iter()
                    .map(|link| format!("{} ({})", link.to, link.kind))
                    .collect();
            }
            let mut usado_por: Vec<String> = Vec::new();
            for caminho in crate::cleaner::metadata_files(workspace.root())? {
                let Ok(outro) = MetaFile::read(&caminho) else {
                    continue;
                };
                if outro.file == rel {
                    continue;
                }
                if outro.links.iter().any(|link| link.to == rel) {
                    usado_por.push(outro.file);
                }
            }
            let mut saida = format!("{rel}\n");
            saida.push_str(&format!("usa ({}):\n", usa.len()));
            for item in &usa {
                saida.push_str(&format!("  {item}\n"));
            }
            saida.push_str(&format!("usado por ({}):\n", usado_por.len()));
            for item in &usado_por {
                saida.push_str(&format!("  {item}\n"));
            }
            Ok(saida)
        }
        "c2_map" => {
            let limite = argumentos
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let descoberta = crate::discover::discover(workspace.root(), config)?;
            let mut saida = String::new();
            for arquivo in descoberta.files.iter().take(limite) {
                saida.push_str(&arquivo.rel);
                saida.push('\n');
            }
            saida.push_str(&format!(
                "({} de {} arquivos)\n",
                descoberta.files.len().min(limite),
                descoberta.files.len()
            ));
            Ok(saida)
        }
        "c2_annotate" => {
            let arquivo = argumentos
                .get("file")
                .and_then(Value::as_str)
                .ok_or_else(|| texto_erro("c2_annotate exige 'file'"))?;
            let texto_nota = argumentos
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| texto_erro("c2_annotate exige 'text'"))?;
            let kind = argumentos.get("kind").and_then(Value::as_str);
            let task = argumentos.get("task").and_then(Value::as_str);
            let model = argumentos.get("model").and_then(Value::as_str);
            let nota =
                crate::notes::append(workspace, arquivo, texto_nota, kind, task, None, model)?;
            Ok(serde_json::to_string_pretty(&json!({
                "ok": true,
                "file": arquivo,
                "note": nota
            }))
            .unwrap_or_default())
        }
        outro => Err(texto_erro(format!(
            "ferramenta desconhecida: {outro} (disponiveis: {})",
            tool_names().join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace(tag: &str) -> (std::path::PathBuf, Workspace, Config) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("c2-mcp-{tag}-{stamp}"));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/a.rs"), "fn a() {}\n").unwrap();
        let ws = Workspace::at(&base);
        ws.init(false).unwrap();
        let config = ws.load_config().unwrap();
        let meta = MetaFile::new(
            "src/a.rs".to_string(),
            "sha256:x".to_string(),
            9,
            1,
            crate::schema::Semantic {
                what: "funcao a".into(),
                why: "existe".into(),
                goal: "rodar".into(),
                summary: "faz a".into(),
                responsibility: "a".into(),
            },
            Vec::new(),
        );
        meta.write_atomic(&base.join("src/a.rs.c2.json")).unwrap();
        (base, ws, config)
    }

    #[test]
    fn initialize_responde_capacidades_e_servidor() {
        let (base, ws, config) = workspace("init");
        let resposta = handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .unwrap();
        let valor: Value = serde_json::from_str(&resposta).unwrap();
        assert_eq!(valor["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(valor["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(valor["result"]["capabilities"]["tools"].is_object());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn tools_list_expoe_as_quatro_ferramentas() {
        let (base, ws, config) = workspace("tools");
        let resposta = handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .unwrap();
        let valor: Value = serde_json::from_str(&resposta).unwrap();
        let nomes: Vec<&str> = valor["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for esperado in tool_names() {
            assert!(nomes.contains(&esperado), "faltou {esperado}: {nomes:?}");
        }
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn tools_call_anotacao_de_agente() {
        let (base, ws, config) = workspace("annotate");
        let resposta = handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"c2_annotate","arguments":{"file":"src/a.rs","text":"invariante: a() nao pode retornar negativo","kind":"invariant"}}}"#,
        )
        .unwrap();
        assert!(resposta.contains("invariant"), "{resposta}");
        let meta = MetaFile::read(&base.join("src/a.rs.c2.json")).unwrap();
        assert_eq!(meta.agent_notes.len(), 1);
        assert_eq!(meta.agent_notes[0].kind, "invariant");
        assert!(meta.agent_notes[0].text.contains("negativo"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn tools_call_executa_grep_e_leitura() {
        let (base, ws, config) = workspace("call");
        let grep = handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"c2_grep","arguments":{"query":"faz a"}}}"#,
        )
        .unwrap();
        assert!(grep.contains("src/a.rs"), "{grep}");

        let leitura = handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"c2_read","arguments":{"file":"src/a.rs"}}}"#,
        )
        .unwrap();
        assert!(leitura.contains("fn a()"), "{leitura}");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn metodo_desconhecido_vira_erro_jsonrpc() {
        let (base, ws, config) = workspace("metodo");
        let resposta = handle(&ws, &config, r#"{"jsonrpc":"2.0","id":9,"method":"x/y"}"#).unwrap();
        let valor: Value = serde_json::from_str(&resposta).unwrap();
        assert_eq!(valor["error"]["code"], -32601);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn notificacao_nao_responde() {
        let (base, ws, config) = workspace("notif");
        assert!(handle(
            &ws,
            &config,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn json_invalido_vira_erro_de_parse() {
        let (base, ws, config) = workspace("parse");
        let resposta = handle(&ws, &config, "nao e json").unwrap();
        let valor: Value = serde_json::from_str(&resposta).unwrap();
        assert_eq!(valor["error"]["code"], -32700);
        fs::remove_dir_all(&base).ok();
    }
}
