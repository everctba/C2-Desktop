//! Anotacoes de agentes (IA principal) anexadas ao `.c2.json`.
//!
//! Escrita restrita: apenas o campo `agent_notes`, com limites de tamanho e
//! quantidade. Nunca altera codigo, `semantic` ou `links`.

use std::path::{Component, Path, PathBuf};

use crate::error::{C2Error, Result};
use crate::schema::{meta_path_for, AgentNote, MetaFile};
use crate::workspace::Workspace;

/// Maximo de anotacoes por arquivo.
pub const MAX_NOTES: usize = 10;
/// Maximo de caracteres por anotacao.
pub const MAX_TEXT_CHARS: usize = 2000;
/// Tipos aceitos.
pub const KINDS: [&str; 4] = ["teleology", "invariant", "gotcha", "note"];

/// Normaliza e valida o caminho relativo, devolvendo o caminho do metadado.
fn caminho_meta(workspace: &Workspace, rel: &str) -> Result<(PathBuf, String)> {
    let rel = rel.trim().trim_start_matches('/').replace('\\', "/");
    if rel.is_empty() {
        return Err(C2Error::InvalidConfig("arquivo vazio".to_string()));
    }
    let caminho = Path::new(&rel);
    if caminho.is_absolute()
        || caminho.components().any(|parte| {
            matches!(
                parte,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(C2Error::InvalidConfig(format!(
            "caminho invalido (relativo ao workspace): `{rel}`"
        )));
    }

    let meta = meta_path_for(&workspace.root().join(&rel));
    if !meta.is_file() {
        return Err(C2Error::InvalidConfig(format!(
            "sem metadado para `{rel}`: rode `c2 reindex` primeiro"
        )));
    }
    let raiz = workspace
        .root()
        .canonicalize()
        .map_err(|erro| C2Error::InvalidConfig(format!("workspace invalido: {erro}")))?;
    let meta_canonico = meta
        .canonicalize()
        .map_err(|erro| C2Error::InvalidConfig(format!("metadado invalido: {erro}")))?;
    if !meta_canonico.starts_with(&raiz) {
        return Err(C2Error::InvalidConfig(
            "metadado fora do workspace".to_string(),
        ));
    }
    Ok((meta, rel))
}

/// Data/hora UTC em ISO-8601 sem dependencias externas.
pub fn agora_iso() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let dias = segundos.div_euclid(86_400);
    let resto = segundos.rem_euclid(86_400);
    let (ano, mes, dia) = civil_from_days(dias);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

fn civil_from_days(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let dia_era = (z - era * 146_097) as u64;
    let ano_era = (dia_era - dia_era / 1460 + dia_era / 36_524 - dia_era / 146_096) / 365;
    let mut ano = ano_era as i64 + era * 400;
    let dia_ano = dia_era - (365 * ano_era + ano_era / 4 - ano_era / 100);
    let mes_pos = (5 * dia_ano + 2) / 153;
    let dia = (dia_ano - (153 * mes_pos + 2) / 5 + 1) as u32;
    let mes = if mes_pos < 10 {
        mes_pos + 3
    } else {
        mes_pos - 9
    } as u32;
    if mes <= 2 {
        ano += 1;
    }
    (ano, mes, dia)
}

/// Anexa uma anotacao ao metadado do arquivo.
pub fn append(
    workspace: &Workspace,
    rel: &str,
    texto: &str,
    kind: Option<&str>,
    task: Option<&str>,
    by: Option<&str>,
    model: Option<&str>,
) -> Result<AgentNote> {
    let (meta_path, rel) = caminho_meta(workspace, rel)?;
    let texto = texto.trim();
    if texto.is_empty() {
        return Err(C2Error::InvalidConfig("anotacao vazia".to_string()));
    }
    if texto.chars().count() > MAX_TEXT_CHARS {
        return Err(C2Error::InvalidConfig(format!(
            "anotacao excede {MAX_TEXT_CHARS} caracteres"
        )));
    }
    let kind = kind.unwrap_or("note").trim().to_ascii_lowercase();
    if !KINDS.contains(&kind.as_str()) {
        return Err(C2Error::InvalidConfig(format!(
            "kind invalido `{kind}` (use: {})",
            KINDS.join(", ")
        )));
    }

    let mut meta = MetaFile::read(&meta_path)?;
    if meta.agent_notes.len() >= MAX_NOTES {
        return Err(C2Error::InvalidConfig(format!(
            "limite de {MAX_NOTES} anotacoes por arquivo atingido em `{rel}`"
        )));
    }
    let nota = AgentNote {
        by: by.unwrap_or("agent").trim().to_string(),
        model: model
            .map(|valor| valor.trim().to_string())
            .filter(|valor| !valor.is_empty()),
        task: task
            .map(|valor| valor.trim().to_string())
            .filter(|valor| !valor.is_empty()),
        at: agora_iso(),
        hash: Some(meta.hash.clone()),
        kind,
        text: texto.to_string(),
    };
    meta.agent_notes.push(nota.clone());
    meta.write_atomic(&meta_path)?;
    Ok(nota)
}

/// Lista anotacoes (de um arquivo ou de todo o workspace).
pub fn list(workspace: &Workspace, filtro: Option<&str>) -> Result<Vec<(String, Vec<AgentNote>)>> {
    let mut resultado = Vec::new();
    match filtro {
        Some(rel) => {
            let (meta_path, rel) = caminho_meta(workspace, rel)?;
            let meta = MetaFile::read(&meta_path)?;
            if !meta.agent_notes.is_empty() {
                resultado.push((rel, meta.agent_notes));
            }
        }
        None => {
            for caminho in crate::cleaner::metadata_files(workspace.root())? {
                let Ok(meta) = MetaFile::read(&caminho) else {
                    continue;
                };
                if meta.agent_notes.is_empty() {
                    continue;
                }
                resultado.push((meta.file, meta.agent_notes));
            }
            resultado.sort_by(|a, b| a.0.cmp(&b.0));
        }
    }
    Ok(resultado)
}

/// Remove uma anotacao (indice 1-based).
pub fn remove(workspace: &Workspace, rel: &str, indice: usize) -> Result<AgentNote> {
    let (meta_path, rel) = caminho_meta(workspace, rel)?;
    let mut meta = MetaFile::read(&meta_path)?;
    if indice == 0 || indice > meta.agent_notes.len() {
        return Err(C2Error::InvalidConfig(format!(
            "indice {indice} fora da faixa (1..={}) em `{rel}`",
            meta.agent_notes.len()
        )));
    }
    let nota = meta.agent_notes.remove(indice - 1);
    meta.write_atomic(&meta_path)?;
    Ok(nota)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{MetaFile, Semantic};

    fn workspace(tag: &str) -> (std::path::PathBuf, Workspace) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("c2-notes-{tag}-{stamp}"));
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src/a.rs"), "fn a() {}\n").unwrap();
        let ws = Workspace::at(&base);
        ws.init(false).unwrap();
        let meta = MetaFile::new(
            "src/a.rs",
            "sha256:abc",
            10,
            1,
            Semantic {
                what: "funcao a".into(),
                why: "existe".into(),
                goal: "rodar".into(),
                summary: "faz a".into(),
                responsibility: "a".into(),
            },
            Vec::new(),
        );
        meta.write_atomic(&base.join("src/a.rs.c2.json")).unwrap();
        (base, ws)
    }

    #[test]
    fn anexa_lista_e_remove() {
        let (base, ws) = workspace("crud");
        let nota = append(
            &ws,
            "src/a.rs",
            "invariante: nunca negativo",
            Some("invariant"),
            Some("tarefa x"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(nota.kind, "invariant");
        assert_eq!(nota.by, "agent");
        assert_eq!(nota.hash.as_deref(), Some("sha256:abc"));
        assert!(nota.at.ends_with('Z'));

        let lista = list(&ws, Some("src/a.rs")).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].1.len(), 1);

        let removida = remove(&ws, "src/a.rs", 1).unwrap();
        assert_eq!(removida.text, "invariante: nunca negativo");
        assert!(list(&ws, Some("src/a.rs")).unwrap().is_empty());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn recusa_caminho_fora_do_workspace() {
        let (base, ws) = workspace("path");
        assert!(append(&ws, "../fora.rs", "x", None, None, None, None).is_err());
        assert!(append(&ws, "src/nao_existe.rs", "x", None, None, None, None).is_err());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn recusa_texto_grande_e_kind_invalido() {
        let (base, ws) = workspace("limites");
        let grande = "a".repeat(MAX_TEXT_CHARS + 1);
        assert!(append(&ws, "src/a.rs", &grande, None, None, None, None).is_err());
        assert!(append(&ws, "src/a.rs", "x", Some("qualquer"), None, None, None).is_err());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn limita_quantidade_de_notas() {
        let (base, ws) = workspace("limite");
        for indice in 0..MAX_NOTES {
            append(
                &ws,
                "src/a.rs",
                &format!("nota {indice}"),
                None,
                None,
                None,
                None,
            )
            .unwrap();
        }
        assert!(append(&ws, "src/a.rs", "excedente", None, None, None, None).is_err());
        std::fs::remove_dir_all(&base).ok();
    }
}
