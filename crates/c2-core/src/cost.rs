//! Custo da inferencia em provedor de nuvem (estimativa, teto e log).

use std::fs::OpenOptions;
use std::io::Write;

use serde::Serialize;

use crate::config::Config;
use crate::error::{C2Error, IoContext, Result};
use crate::indexer::{IndexReport, PlanItem};
use crate::workspace::Workspace;

/// Estimativa grosseira de tokens de saida por arquivo (sem historico).
pub const COMPLETION_ESTIMATE_TOKENS: u64 = 400;

/// Custo em USD de uma chamada.
pub fn custo(
    prompt_tokens: u64,
    completion_tokens: u64,
    preco_entrada_mtok: f64,
    preco_saida_mtok: f64,
) -> f64 {
    prompt_tokens as f64 / 1_000_000.0 * preco_entrada_mtok
        + completion_tokens as f64 / 1_000_000.0 * preco_saida_mtok
}

/// Estimativa do custo do plano (antes de chamar o modelo).
///
/// Heuristica: prompt ~ (instrucoes + min(tamanho, max_input_chars)) / 4
/// tokens; saida ~ [COMPLETION_ESTIMATE_TOKENS]. Serve para ordem de grandeza,
/// nao para contabilidade exata.
pub fn estimar_plano(config: &Config, items: &[PlanItem]) -> f64 {
    let preco_entrada = config.model.cost_input_per_mtok;
    let preco_saida = config.model.cost_output_per_mtok;
    if preco_entrada <= 0.0 && preco_saida <= 0.0 {
        return 0.0;
    }
    let instrucoes = config.index.prompt.chars().count() + 80;
    let mut total = 0.0;
    for item in items {
        if !matches!(
            item.decision,
            crate::indexer::Decision::New | crate::indexer::Decision::Changed
        ) {
            continue;
        }
        let conteudo = (item.size as usize).min(config.model.max_input_chars);
        let prompt_tokens = ((instrucoes + conteudo) / 4) as u64;
        total += custo(
            prompt_tokens,
            COMPLETION_ESTIMATE_TOKENS,
            preco_entrada,
            preco_saida,
        );
    }
    total
}

#[derive(Serialize)]
struct Entrada {
    at: String,
    provider: String,
    model: String,
    calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    cost_usd: f64,
    estimated_cost_usd: f64,
    cost_capped: bool,
    latency_ms: u64,
}

/// Anexa a execucao ao `.c2/logs/cost.jsonl`.
pub fn registrar(workspace: &Workspace, config: &Config, report: &IndexReport) -> Result<()> {
    if report.calls == 0 {
        return Ok(());
    }
    let dir = workspace.root().join(".c2").join("logs");
    std::fs::create_dir_all(&dir).at(&dir)?;
    let caminho = dir.join("cost.jsonl");
    let entrada = Entrada {
        at: crate::notes::agora_iso(),
        provider: config.model.provider.label().to_string(),
        model: config.model.name.clone(),
        calls: report.calls,
        prompt_tokens: report.prompt_tokens,
        completion_tokens: report.completion_tokens,
        cost_usd: report.cost_usd,
        estimated_cost_usd: report.estimated_cost_usd,
        cost_capped: report.cost_capped,
        latency_ms: report.latency_ms,
    };
    let mut arquivo = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&caminho)
        .at(&caminho)?;
    let linha = serde_json::to_string(&entrada).map_err(C2Error::from)?;
    writeln!(arquivo, "{linha}").at(&caminho)?;
    Ok(())
}

/// Resumo textual do custo para CLI/UI (vazio quando nao calculado).
pub fn resumo(report: &IndexReport) -> String {
    if report.cost_usd <= 0.0 && report.estimated_cost_usd <= 0.0 {
        return String::new();
    }
    let teto = if report.cost_capped {
        " · TETO ATINGIDO"
    } else {
        ""
    };
    format!(
        "custo: US$ {:.4} (estimado antes: US$ {:.4}){}",
        report.cost_usd, report.estimated_cost_usd, teto
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calcula_custo_por_milhao_de_tokens() {
        let valor = custo(1_000_000, 1_000_000, 0.15, 0.60);
        assert!((valor - 0.75).abs() < 1e-9);
        assert!(custo(0, 0, 1.0, 1.0) == 0.0);
    }
}
