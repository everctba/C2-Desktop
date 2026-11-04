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
    let mut comando = Command::new("nvidia-smi");
    comando.args([
        "--query-gpu=name,memory.total,memory.used",
        "--format=csv,noheader,nounits",
    ]);
    crate::process::sem_janela(&mut comando);
    let output = comando.output().ok()?;

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

/// Estimativa de memoria para carregar o modelo com um contexto.
#[derive(Debug, Clone, Serialize)]
pub struct UsageEstimate {
    /// Peso do arquivo do modelo.
    pub model_mb: u64,
    /// KV cache estimado para o contexto pedido.
    pub kv_mb: u64,
    /// Overhead de runtime/contexto CUDA/Vulkan.
    pub overhead_mb: u64,
    /// Total estimado.
    pub total_mb: u64,
    /// VRAM livre no momento (0 quando nao ha GPU detectada).
    pub available_mb: u64,
    pub fit: VramFit,
}

/// Estima o uso de VRAM: pesos + overhead (5%) + KV cache + 256 MiB fixos.
///
/// O KV cache e aproximado por proporcao: um modelo de 3,2 GB com 8k de
/// contexto consome ~0,7 GB, ou seja `ctx/8192 * model_mb * 0,22`. E uma
/// heuristica de triagem para a barra da UI, nao uma medida do llama.cpp.
pub fn estimate_usage(size_bytes: u64, ctx_size: u32, gpu: Option<&GpuInfo>) -> UsageEstimate {
    let model_mb = size_bytes / (1024 * 1024);
    let overhead_mb = (model_mb / 20).max(64);
    let kv_mb = ((ctx_size as f64 / 8192.0) * (model_mb as f64 * 0.22)).round() as u64;
    let total_mb = model_mb + overhead_mb + kv_mb + 256;
    let available_mb = gpu.map(|gpu| gpu.free_mb).unwrap_or(0);

    let free_com_folga = available_mb.saturating_sub(available_mb / 10);
    let fit = match gpu {
        None => VramFit::Unknown,
        Some(_) if total_mb <= free_com_folga => VramFit::Fits,
        Some(_) if total_mb <= available_mb => VramFit::Tight,
        Some(_) => VramFit::TooBig,
    };

    UsageEstimate {
        model_mb,
        kv_mb,
        overhead_mb,
        total_mb,
        available_mb,
        fit,
    }
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
    fn estimativa_cresce_com_contexto() {
        let tamanho = 3_200 * 1024 * 1024;
        let gpu_ = gpu(8000);
        let curto = estimate_usage(tamanho, 4096, Some(&gpu_));
        let longo = estimate_usage(tamanho, 16384, Some(&gpu_));
        assert!(longo.kv_mb > curto.kv_mb);
        assert!(longo.total_mb > curto.total_mb);
        assert!(curto.total_mb > curto.model_mb);
        assert_eq!(curto.available_mb, 8000);
    }

    #[test]
    fn estimativa_sem_gpu_e_unknown() {
        let estimativa = estimate_usage(1024 * 1024 * 1024, 8192, None);
        assert_eq!(estimativa.fit, VramFit::Unknown);
        assert_eq!(estimativa.available_mb, 0);
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
