//! Leitura do cabecalho GGUF (bead `c2-e14.2`).
//!
//! Nao carrega o modelo: le apenas o cabecalho e os metadados necessarios para
//! a janela mostrar nome, arquitetura, quantizacao e tamanho antes de o usuario
//! confirmar. O arquivo NUNCA e copiado — so o caminho e gravado na config.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

use crate::error::{C2Error, IoContext, Result};

const MAGIC: &[u8; 4] = b"GGUF";
const MAX_KEY_BYTES: u64 = 1024;
const MAX_STRING_BYTES: u64 = 64 * 1024;

/// Informacoes uteis de um arquivo GGUF.
#[derive(Debug, Clone, Serialize)]
pub struct GgufInfo {
    pub path: String,
    pub size_bytes: u64,
    pub version: u32,
    pub tensor_count: u64,
    pub architecture: Option<String>,
    pub name: Option<String>,
    /// Rotulo legivel da quantizacao (Q4_K_M, F16...).
    pub quantization: String,
    /// Valor bruto de `general.file_type`, quando o modelo traz o campo.
    pub file_type: Option<u32>,
    /// `general.size_label` (ex.: "Q4_K_M"), comum em GGUF do LM Studio.
    pub size_label: Option<String>,
    /// Contexto de treino, quando presente (`{arch}.context_length`).
    pub context_length: Option<u64>,
    /// Numero de camadas (`{arch}.block_count`).
    pub block_count: Option<u32>,
}

/// Le o cabecalho do GGUF. Erro se o arquivo nao existir ou nao for GGUF.
pub fn read_info(path: &Path) -> Result<GgufInfo> {
    let file = File::open(path).at(path)?;
    let size_bytes = file.metadata().at(path)?.len();
    let mut reader = BufReader::with_capacity(64 * 1024, file);

    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic).at(path)?;
    if &magic != MAGIC {
        return Err(C2Error::InvalidConfig(format!(
            "nao e um arquivo GGUF (magic invalido): {}",
            path.display()
        )));
    }

    let version = read_u32(&mut reader, path)?;
    let tensor_count = read_u64(&mut reader, path)?;
    let kv_count = read_u64(&mut reader, path)?;

    let mut values: HashMap<String, Value> = HashMap::new();
    let mut lidos = 0u64;
    while lidos < kv_count && lidos < 4096 {
        let key = match read_string(&mut reader, path, MAX_KEY_BYTES) {
            Ok(key) => key,
            Err(_) => break,
        };
        let tipo = read_u32(&mut reader, path)?;
        let Some(value) = read_value(&mut reader, path, tipo)? else {
            break;
        };
        values.insert(key, value);
        lidos += 1;

        // `general.*` e `{arch}.*` vem antes do tokenizador; paramos assim que
        // temos o essencial para nao ler arrays de centenas de MB.
        let tem_base = values.contains_key("general.architecture")
            && values.contains_key("general.name")
            && values.contains_key("general.size_label");
        let tem_arq = values
            .get("general.architecture")
            .and_then(Value::as_str)
            .map(|arch| {
                values.contains_key(&format!("{arch}.context_length"))
                    && values.contains_key(&format!("{arch}.block_count"))
            })
            .unwrap_or(false);
        if tem_base && tem_arq {
            break;
        }
    }

    let file_type = values.get("general.file_type").and_then(Value::as_u32);
    let size_label = values
        .get("general.size_label")
        .and_then(Value::as_str)
        .map(str::to_string);
    let architecture = values
        .get("general.architecture")
        .and_then(Value::as_str)
        .map(str::to_string);

    let context_length = architecture
        .as_ref()
        .and_then(|arch| values.get(&format!("{arch}.context_length")))
        .and_then(Value::as_u64);
    let block_count = architecture
        .as_ref()
        .and_then(|arch| values.get(&format!("{arch}.block_count")))
        .and_then(Value::as_u32);

    Ok(GgufInfo {
        path: path.display().to_string(),
        size_bytes,
        version,
        tensor_count,
        quantization: resolve_quantization(file_type, size_label.as_deref(), path),
        architecture,
        name: values
            .get("general.name")
            .and_then(Value::as_str)
            .map(str::to_string),
        file_type,
        size_label,
        context_length,
        block_count,
    })
}

/// Quantizacao: `general.file_type` > `general.size_label` > nome do arquivo.
fn resolve_quantization(
    file_type: Option<u32>,
    size_label: Option<&str>,
    path: &Path,
) -> String {
    if file_type.is_some() {
        return quantization_label(file_type).to_string();
    }

    if let Some(label) = size_label {
        if looks_like_quant(label) {
            return label.to_string();
        }
    }

    let nome = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    for candidato in [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_1", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_1", "Q4_0",
        "Q3_K_L", "Q3_K_M", "Q3_K_S", "Q2_K", "BF16", "F16", "F32",
    ] {
        if nome.contains(candidato) {
            return candidato.to_string();
        }
    }
    "desconhecida".to_string()
}

fn looks_like_quant(label: &str) -> bool {
    let upper = label.to_uppercase();
    ["Q2_", "Q3_", "Q4_", "Q5_", "Q6_", "Q8_", "IQ", "BF16", "F16", "F32"]
        .iter()
        .any(|prefixo| upper.starts_with(prefixo))
}

/// Rotulo de `general.file_type` (enum `llama_ftype` do llama.cpp).
pub fn quantization_label(file_type: Option<u32>) -> &'static str {
    match file_type {
        Some(0) => "F32",
        Some(1) => "F16",
        Some(2) => "Q4_0",
        Some(3) => "Q4_1",
        Some(7) => "Q8_0",
        Some(8) => "Q5_0",
        Some(9) => "Q5_1",
        Some(10) => "Q2_K",
        Some(11) => "Q3_K_S",
        Some(12) => "Q3_K_M",
        Some(13) => "Q3_K_L",
        Some(14) => "Q4_K_S",
        Some(15) => "Q4_K_M",
        Some(16) => "Q5_K_S",
        Some(17) => "Q5_K_M",
        Some(18) => "Q6_K",
        Some(19) => "IQ2_XXS",
        Some(20) => "IQ2_XS",
        Some(21) => "Q2_K_S",
        Some(22) => "IQ3_XS",
        Some(23) => "IQ3_XXS",
        Some(24) => "IQ1_S",
        Some(25) => "IQ4_NL",
        Some(26) => "IQ3_S",
        Some(27) => "IQ3_M",
        Some(28) => "IQ2_S",
        Some(29) => "IQ2_M",
        Some(30) => "IQ4_XS",
        Some(31) => "IQ1_M",
        Some(32) => "BF16",
        Some(_) => "desconhecida",
        None => "desconhecida",
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Value {
    UInt(u64),
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    fn as_u64(&self) -> Option<u64> {
        match self {
            Value::UInt(value) => Some(*value),
            Value::Int(value) => Some(*value as u64),
            _ => None,
        }
    }

    fn as_u32(&self) -> Option<u32> {
        match self {
            Value::UInt(value) => Some(*value as u32),
            Value::Int(value) => Some(*value as u32),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(value) => Some(value),
            _ => None,
        }
    }
}

fn read_u8<R: Read>(reader: &mut R, path: &Path) -> Result<u8> {
    let mut buffer = [0u8; 1];
    reader.read_exact(&mut buffer).at(path)?;
    Ok(buffer[0])
}

fn read_u32<R: Read>(reader: &mut R, path: &Path) -> Result<u32> {
    let mut buffer = [0u8; 4];
    reader.read_exact(&mut buffer).at(path)?;
    Ok(u32::from_le_bytes(buffer))
}

fn read_u64<R: Read>(reader: &mut R, path: &Path) -> Result<u64> {
    let mut buffer = [0u8; 8];
    reader.read_exact(&mut buffer).at(path)?;
    Ok(u64::from_le_bytes(buffer))
}

fn read_string<R: Read>(reader: &mut R, path: &Path, max: u64) -> Result<String> {
    let len = read_u64(reader, path)?;
    if len > max {
        return Err(C2Error::InvalidConfig(format!(
            "string GGUF grande demais ({len} bytes)"
        )));
    }
    let mut buffer = vec![0u8; len as usize];
    reader.read_exact(&mut buffer).at(path)?;
    Ok(String::from_utf8_lossy(&buffer).to_string())
}

/// Le um valor GGUF. Arrays sao pulados (nao interessam ao metadado exibido).
fn read_value<R: Read + Seek>(reader: &mut R, path: &Path, tipo: u32) -> Result<Option<Value>> {
    Ok(Some(match tipo {
        0 => Value::UInt(u64::from(read_u8(reader, path)?)),
        1 => Value::Int(i64::from(read_u8(reader, path)? as i8)),
        2 => Value::UInt(u64::from(u16::from_le_bytes(read_n(reader, path)?))),
        3 => Value::Int(i64::from(i16::from_le_bytes(read_n(reader, path)?))),
        4 => Value::UInt(u64::from(read_u32(reader, path)?)),
        5 => Value::Int(i64::from(i32::from_le_bytes(read_n(reader, path)?))),
        6 => Value::Float(f32::from_le_bytes(read_n(reader, path)?) as f64),
        7 => Value::Bool(read_u8(reader, path)? != 0),
        8 => Value::Str(read_string(reader, path, MAX_STRING_BYTES)?),
        9 => {
            let elemento = read_u32(reader, path)?;
            let quantidade = read_u64(reader, path)?;
            skip_array(reader, path, elemento, quantidade)?;
            return Ok(None);
        }
        10 => Value::UInt(read_u64(reader, path)?),
        11 => Value::Int(i64::from_le_bytes(read_n(reader, path)?)),
        12 => Value::Float(f64::from_le_bytes(read_n(reader, path)?)),
        outro => {
            return Err(C2Error::InvalidConfig(format!(
                "tipo de metadado GGUF desconhecido: {outro}"
            )))
        }
    }))
}

fn read_n<R: Read, const N: usize>(reader: &mut R, path: &Path) -> Result<[u8; N]> {
    let mut buffer = [0u8; N];
    reader.read_exact(&mut buffer).at(path)?;
    Ok(buffer)
}

fn skip_array<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    elemento: u32,
    quantidade: u64,
) -> Result<()> {
    let tamanho = match elemento {
        0..=1 | 7 => 1u64,
        2..=3 => 2,
        4..=6 => 4,
        10..=12 => 8,
        8 => {
            // strings precisam ser lidas uma a uma para avancar o cursor
            for _ in 0..quantidade {
                let len = read_u64(reader, path)?;
                if len > MAX_STRING_BYTES {
                    return Err(C2Error::InvalidConfig(
                        "array de strings GGUF grande demais".to_string(),
                    ));
                }
                reader
                    .seek(SeekFrom::Current(len as i64))
                    .at(path)?;
            }
            return Ok(());
        }
        outro => {
            return Err(C2Error::InvalidConfig(format!(
                "array GGUF com tipo desconhecido: {outro}"
            )))
        }
    };
    reader
        .seek(SeekFrom::Current((tamanho * quantidade) as i64))
        .at(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Monta um GGUF minimo valido com os metadados general.*.
    fn gguf_bytes(name: &str, architecture: &str, file_type: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&2u64.to_le_bytes()); // tensores
        bytes.extend_from_slice(&3u64.to_le_bytes()); // kv

        for (key, value) in [("general.name", name), ("general.architecture", architecture)] {
            bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
            bytes.extend_from_slice(key.as_bytes());
            bytes.extend_from_slice(&8u32.to_le_bytes()); // string
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        bytes.extend_from_slice(&(b"general.file_type".len() as u64).to_le_bytes());
        bytes.extend_from_slice(b"general.file_type");
        bytes.extend_from_slice(&4u32.to_le_bytes()); // u32
        bytes.extend_from_slice(&file_type.to_le_bytes());
        bytes
    }

    fn tmp(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("c2-gguf-{tag}-{stamp}.gguf"));
        let mut file = File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn le_nome_arquitetura_e_quantizacao() {
        let path = tmp("ok", &gguf_bytes("gemma-4-E2B-it", "gemma", 15));
        let info = read_info(&path).unwrap();
        assert_eq!(info.version, 3);
        assert_eq!(info.tensor_count, 2);
        assert_eq!(info.name.as_deref(), Some("gemma-4-E2B-it"));
        assert_eq!(info.architecture.as_deref(), Some("gemma"));
        assert_eq!(info.quantization, "Q4_K_M");
        assert!(info.size_bytes > 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn recusa_arquivo_que_nao_e_gguf() {
        let path = tmp("bad", b"nao sou gguf");
        let err = read_info(&path).unwrap_err();
        assert!(err.to_string().contains("GGUF"), "{err}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn quantizacao_por_size_label_ou_nome_do_arquivo() {
        let path = std::path::Path::new("modelo-Q4_K_M.gguf");
        assert_eq!(resolve_quantization(None, Some("Q4_K_M"), path), "Q4_K_M");
        assert_eq!(
            resolve_quantization(None, None, std::path::Path::new("gemma-4-E2B-it-Q4_K_M.gguf")),
            "Q4_K_M"
        );
        assert_eq!(
            resolve_quantization(None, Some("pequeno"), std::path::Path::new("a.gguf")),
            "desconhecida"
        );
    }

    #[test]
    fn rotulos_de_quantizacao_conhecidos() {
        assert_eq!(quantization_label(Some(15)), "Q4_K_M");
        assert_eq!(quantization_label(Some(1)), "F16");
        assert_eq!(quantization_label(None), "desconhecida");
        assert_eq!(quantization_label(Some(999)), "desconhecida");
    }
}
