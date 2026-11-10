//! Telemetria local de uso (MCP/CLI/desktop). Nunca sai da maquina.
//!
//! Task `c2-e54`: registra chamadas, leituras efetivas de metadados e
//! anotacoes. O log fica em `.c2/logs/usage.jsonl` e e rotacionado.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};
use crate::notes;
use crate::workspace::Workspace;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub schema_version: u32,
    pub at: String,
    /// `mcp`, `cli` ou `desktop`.
    pub actor: String,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default)]
    pub meta_reads: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub ok: bool,
}

impl UsageEvent {
    pub fn new(actor: &str, tool: &str) -> Self {
        Self {
            schema_version: 1,
            at: notes::agora_iso(),
            actor: actor.to_string(),
            tool: tool.to_string(),
            file: None,
            meta_reads: 0,
            result_count: None,
            annotation_kind: None,
            duration_ms: None,
            ok: true,
        }
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn meta_reads(mut self, quantidade: usize) -> Self {
        self.meta_reads = quantidade;
        self
    }

    pub fn annotation(mut self, kind: impl Into<String>) -> Self {
        self.annotation_kind = Some(kind.into());
        self
    }

    pub fn result_count(mut self, quantidade: usize) -> Self {
        self.result_count = Some(quantidade);
        self
    }

    pub fn duration(mut self, millis: u64) -> Self {
        self.duration_ms = Some(millis);
        self
    }

    pub fn failed(mut self) -> Self {
        self.ok = false;
        self
    }
}

fn usage_path(workspace: &Workspace) -> std::path::PathBuf {
    workspace.logs_dir().join("usage.jsonl")
}

/// Anexa um evento. Falha nunca deve derrubar a operacao principal.
pub fn record(workspace: &Workspace, event: &UsageEvent) -> Result<()> {
    let path = usage_path(workspace);
    fs::create_dir_all(workspace.logs_dir()).at(&workspace.logs_dir())?;
    if fs::metadata(&path)
        .map(|meta| meta.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let rotated = path.with_extension("jsonl.1");
        let _ = fs::rename(&path, rotated);
    }
    let linha = serde_json::to_string(event)?;
    let mut arquivo = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .at(&path)?;
    writeln!(arquivo, "{linha}").at(&path)?;
    Ok(())
}

/// Le os eventos (mais recentes primeiro), limitado para nao carregar um log
/// gigante na UI.
pub fn events(workspace: &Workspace, limite: usize) -> Result<Vec<UsageEvent>> {
    let path = usage_path(workspace);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let texto = fs::read_to_string(&path).at(&path)?;
    let mut saida: Vec<UsageEvent> = texto
        .lines()
        .filter_map(|linha| serde_json::from_str(linha).ok())
        .collect();
    saida.sort_by(|a, b| b.at.cmp(&a.at));
    saida.truncate(limite.max(1));
    Ok(saida)
}

/// Contagem simples para a pagina de Memoria.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageSummary {
    pub calls: usize,
    pub meta_reads: usize,
    pub annotations: usize,
    pub code_reads: usize,
    /// Chamada de contexto (grep/related/similar/brief/map) antes da primeira
    /// leitura de codigo. Indica adocao sem lembrete explicito.
    pub preflight_before_code: bool,
    pub by_actor: BTreeMap<String, usize>,
    pub by_tool: BTreeMap<String, usize>,
}

pub fn summarize(eventos: &[UsageEvent]) -> UsageSummary {
    let mut resumo = UsageSummary {
        calls: eventos.len(),
        ..UsageSummary::default()
    };
    let mut ordenados = eventos.to_vec();
    ordenados.sort_by(|a, b| a.at.cmp(&b.at));
    let mut primeira_leitura_codigo: Option<usize> = None;
    let mut primeira_chamada_contexto: Option<usize> = None;
    for (indice, evento) in ordenados.iter().enumerate() {
        resumo.meta_reads += evento.meta_reads;
        if evento.tool == "c2_annotate" || evento.annotation_kind.is_some() {
            resumo.annotations += 1;
        }
        if evento.tool == "c2_read" || evento.tool == "read" {
            resumo.code_reads += 1;
            if primeira_leitura_codigo.is_none() {
                primeira_leitura_codigo = Some(indice);
            }
        }
        if matches!(
            evento.tool.as_str(),
            "c2_grep" | "c2_related" | "c2_similar" | "c2_brief" | "c2_map"
        ) && primeira_chamada_contexto.is_none()
        {
            primeira_chamada_contexto = Some(indice);
        }
        *resumo.by_actor.entry(evento.actor.clone()).or_default() += 1;
        *resumo.by_tool.entry(evento.tool.clone()).or_default() += 1;
    }
    resumo.preflight_before_code = match (primeira_chamada_contexto, primeira_leitura_codigo) {
        (Some(contexto), Some(leitura)) => contexto < leitura,
        (Some(_), None) => true,
        _ => false,
    };
    resumo
}

/// Caminho do log (diagnostico).
pub fn path_for(workspace: &Workspace) -> std::path::PathBuf {
    usage_path(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ws(tag: &str) -> (std::path::PathBuf, Workspace) {
        let dir = std::env::temp_dir().join(format!(
            "c2-usage-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join(".c2")).unwrap();
        (dir.clone(), Workspace::at(dir))
    }

    #[test]
    fn registra_e_resume() {
        let (dir, workspace) = ws("rec");
        record(
            &workspace,
            &UsageEvent::new("mcp", "c2_read")
                .file("src/a.rs")
                .meta_reads(1),
        )
        .unwrap();
        record(
            &workspace,
            &UsageEvent::new("mcp", "c2_annotate")
                .file("src/a.rs")
                .annotation("gotcha"),
        )
        .unwrap();
        let eventos = events(&workspace, 10).unwrap();
        assert_eq!(eventos.len(), 2);
        let resumo = summarize(&eventos);
        assert_eq!(resumo.meta_reads, 1);
        assert_eq!(resumo.annotations, 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn preflight_antes_da_leitura() {
        let (dir, workspace) = ws("preflight");
        record(
            &workspace,
            &UsageEvent::new("mcp", "c2_grep").result_count(2),
        )
        .unwrap();
        record(
            &workspace,
            &UsageEvent::new("mcp", "c2_read")
                .file("src/a.rs")
                .meta_reads(1),
        )
        .unwrap();
        let resumo = summarize(&events(&workspace, 10).unwrap());
        assert_eq!(resumo.code_reads, 1);
        assert_eq!(resumo.meta_reads, 1);
        assert!(resumo.preflight_before_code);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn le_log_vazio() {
        let (dir, workspace) = ws("empty");
        assert!(events(&workspace, 10).unwrap().is_empty());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rotaciona_log_grande() {
        let (dir, workspace) = ws("rot");
        let path = path_for(&workspace);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();
        record(&workspace, &UsageEvent::new("cli", "read")).unwrap();
        assert!(path.with_extension("jsonl.1").is_file());
        assert!(path.is_file());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn evento_tem_schema_version() {
        let evento = UsageEvent::new("cli", "grep");
        assert_eq!(evento.schema_version, 1);
        assert!(evento.at.ends_with('Z'));
    }
}
