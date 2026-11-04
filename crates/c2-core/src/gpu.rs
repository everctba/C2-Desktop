//! Deteccao de GPU para o seletor de modelo (bead `c2-e14.2`).
//!
//! Usa `nvidia-smi` quando disponivel. Sem GPU/NVIDIA, devolve `None` e a
//! janela simplesmente nao mostra o veredito de VRAM.

use std::process::Command;

use serde::Serialize;

/// Memoria da GPU, em MiB.
#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
}

/// Consulta a primeira GPU NVIDIA.
pub fn detect() -> Option<GpuInfo> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?;
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next()?.to_string();
    let total_mb = parts.next()?.parse::<u64>().ok()?;
    let used_mb = parts.next()?.parse::<u64>().ok()?;

    Some(GpuInfo {
        name,
        total_mb,
        used_mb,
        free_mb: total_mb.saturating_sub(used_mb),
    })
}

/// Veredito de encaixe do modelo na VRAM livre.
///
/// Estimativa: tamanho do arquivo + 10% de overhead + 512 MiB para o KV cache
/// com contexto moderado. E uma heuristica de triagem, nao uma garantia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VramFit {
    Fits,
    Tight,
    TooBig,
    Unknown,
}

pub fn vram_fit(size_bytes: u64, gpu: Option<&GpuInfo>) -> VramFit {
    let Some(gpu) = gpu else {
        return VramFit::Unknown;
    };
    let file_mb = size_bytes / (1024 * 1024);
    let needed_mb = file_mb + file_mb / 10 + 512;

    if needed_mb <= gpu.free_mb.saturating_sub(gpu.free_mb / 10) {
        VramFit::Fits
    } else if needed_mb <= gpu.free_mb {
        VramFit::Tight
    } else {
        VramFit::TooBig
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(free_mb: u64) -> GpuInfo {
        GpuInfo {
            name: "teste".to_string(),
            total_mb: free_mb,
            used_mb: 0,
            free_mb,
        }
    }

    #[test]
    fn classifica_encaixe() {
        assert_eq!(
            vram_fit(3 * 1024 * 1024 * 1024, Some(&gpu(7000))),
            VramFit::Fits
        );
        assert_eq!(
            vram_fit(6 * 1024 * 1024 * 1024, Some(&gpu(6000))),
            VramFit::TooBig
        );
        assert_eq!(vram_fit(1024 * 1024 * 1024, None), VramFit::Unknown);
    }
}
