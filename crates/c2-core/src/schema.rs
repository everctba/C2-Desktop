//! Schema v1 do metadado `.c2.json`.
//!
//! Regras:
//! - leitura tolerante: aceita metadado v1, o formato do prototipo bash e
//!   campos semanticos soltos na raiz (ver [`MetaFile::from_value`]);
//! - escrita sempre atomica (arquivo temporario no mesmo diretorio + rename);
//! - nunca grava metadado vazio: [`MetaFile::validate`] recusa semantica vazia.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{C2Error, IoContext, Result};

/// Versao atual do schema.
pub const SCHEMA_VERSION: u32 = 1;

/// Sufixo do arquivo de metadado.
pub const META_SUFFIX: &str = ".c2.json";

/// Caminho do metadado correspondente a um arquivo-fonte.
pub fn meta_path_for(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_os_string();
    name.push(META_SUFFIX);
    PathBuf::from(name)
}

/// Verdadeiro quando o caminho aponta para um metadado `.c2.json`.
pub fn is_meta_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(META_SUFFIX))
}

/// Caminho do arquivo-fonte a partir do caminho do metadado.
pub fn source_path_for(meta: &Path) -> PathBuf {
    let text = meta.to_string_lossy();
    let trimmed = text.strip_suffix(META_SUFFIX).unwrap_or(&text);
    PathBuf::from(trimmed)
}

/// Semantica do arquivo (schema v1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Semantic {
    /// Do que se trata o arquivo.
    pub what: String,
    /// Por que ele existe.
    pub why: String,
    /// Que objetivo ele garante.
    pub goal: String,
    /// Responsabilidade em uma frase.
    pub summary: String,
    /// O que e de sua propriedade.
    pub responsibility: String,
}

impl Semantic {
    /// Verdadeiro quando nenhum campo foi preenchido.
    pub fn is_empty(&self) -> bool {
        self.what.is_empty()
            && self.why.is_empty()
            && self.goal.is_empty()
            && self.summary.is_empty()
            && self.responsibility.is_empty()
    }
}

/// Aresta do grafo estatico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    /// Caminho do arquivo de destino, relativo a raiz do workspace.
    pub to: String,
    /// `imports`, `includes`, `mod`, `require`, `extends`...
    #[serde(rename = "type")]
    pub kind: String,
}

impl Link {
    pub fn new(to: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            kind: kind.into(),
        }
    }
}

/// Metadado de um arquivo do projeto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetaFile {
    pub schema_version: u32,
    /// Caminho relativo a raiz do workspace, sempre com `/`.
    pub file: String,
    /// `sha256:<hex>` do conteudo analisado.
    pub hash: String,
    /// Tamanho em bytes do conteudo analisado.
    pub size: u64,
    /// mtime (segundos desde a epoch) do conteudo analisado.
    pub mtime: i64,
    pub semantic: Semantic,
    pub links: Vec<Link>,
}

/// Remove o BOM UTF-8, se presente (alguns editores do Windows gravam).
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

impl MetaFile {
    pub fn new(
        file: impl Into<String>,
        hash: impl Into<String>,
        size: u64,
        mtime: i64,
        semantic: Semantic,
        links: Vec<Link>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            file: file.into(),
            hash: hash.into(),
            size,
            mtime,
            semantic,
            links,
        }
    }

    /// Recusa metadado sem semantica: e o bug do prototipo bash que gravava vazio.
    pub fn validate(&self) -> Result<()> {
        if self.semantic.is_empty() {
            return Err(C2Error::InvalidConfig(format!(
                "metadado de `{}` sem semantica (what/why/goal/summary/responsibility vazios)",
                self.file
            )));
        }
        if self.hash.is_empty() {
            return Err(C2Error::InvalidConfig(format!(
                "metadado de `{}` sem hash",
                self.file
            )));
        }
        Ok(())
    }

    /// Verdadeiro quando o metadado descreve exatamente o conteudo informado.
    pub fn is_fresh(&self, hash: &str, size: u64) -> bool {
        self.hash == hash && self.size == size
    }

    /// Leitura tolerante a partir de um `Value`.
    pub fn from_value(value: &Value) -> Self {
        let root = value.as_object();
        let semantic_obj = value.get("semantic").and_then(Value::as_object);

        let pick = |key: &str| -> String {
            semantic_obj
                .and_then(|m: &Map<String, Value>| m.get(key))
                .and_then(Value::as_str)
                .or_else(|| root.and_then(|m| m.get(key)).and_then(Value::as_str))
                .unwrap_or_default()
                .to_string()
        };

        // O prototipo bash usava `when`; nao existe no schema v1, mas e aceito.
        // Como nao ha campo equivalente, o conteudo vira `what` quando `what`
        // esta vazio — migrar nao pode perder informacao (c2-e11t4).
        let legacy_when = {
            let value = semantic_obj
                .and_then(|m: &Map<String, Value>| m.get("when"))
                .and_then(Value::as_str)
                .or_else(|| root.and_then(|m| m.get("when")).and_then(Value::as_str));
            value.map(str::to_string)
        };

        // `reasoning` e o nome usado pelo schema antigo para o "por que".
        let why = {
            let value = pick("why");
            if value.is_empty() {
                pick("reasoning")
            } else {
                value
            }
        };

        let what = {
            let value = pick("what");
            if value.is_empty() {
                legacy_when.clone().unwrap_or_default()
            } else {
                value
            }
        };

        let semantic = Semantic {
            what,
            why,
            goal: pick("goal"),
            summary: pick("summary"),
            responsibility: pick("responsibility"),
        };

        let mut links = Vec::new();
        for key in ["links", "dependencies", "uses"] {
            let Some(items) = value.get(key).and_then(Value::as_array) else {
                continue;
            };
            for item in items {
                match item {
                    Value::String(to) => links.push(Link::new(to.clone(), "imports")),
                    Value::Object(_) => {
                        let to = item
                            .get("to")
                            .or_else(|| item.get("path"))
                            .or_else(|| item.get("file"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if to.is_empty() {
                            continue;
                        }
                        let kind = item
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("imports");
                        links.push(Link::new(to, kind));
                    }
                    _ => {}
                }
            }
        }
        links.sort_by(|a, b| {
            (a.to.as_str(), a.kind.as_str()).cmp(&(b.to.as_str(), b.kind.as_str()))
        });
        links.dedup();

        MetaFile {
            schema_version: value
                .get("schema_version")
                .and_then(Value::as_u64)
                .map_or(SCHEMA_VERSION, |v| v as u32),
            file: value
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            hash: value
                .get("hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
            mtime: value.get("mtime").and_then(Value::as_i64).unwrap_or(0),
            semantic,
            links,
        }
    }

    /// Parse tolerante de bytes JSON (aceita BOM UTF-8).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(strip_bom(bytes))?;
        Ok(Self::from_value(&value))
    }

    /// Le o metadado do disco.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).at(path)?;
        Self::parse(&bytes).map_err(|source| match source {
            C2Error::Json(source) => C2Error::InvalidMeta {
                path: path.to_path_buf(),
                source,
            },
            other => other,
        })
    }

    /// Serializa como JSON indentado.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(C2Error::from)
    }

    /// Escrita atomica: temporario no mesmo diretorio + rename.
    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .ok_or_else(|| C2Error::InvalidConfig(format!("caminho invalido: {path:?}")))?;
        fs::create_dir_all(parent).at(parent)?;

        let json = self.to_json()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let tmp = parent.join(format!(".c2-tmp-{}-{stamp}", std::process::id()));

        {
            let mut file = fs::File::create(&tmp).at(&tmp)?;
            file.write_all(json.as_bytes()).at(&tmp)?;
            file.write_all(b"\n").at(&tmp)?;
            file.sync_all().at(&tmp)?;
        }

        if let Err(err) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(C2Error::io(path, err));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("c2-schema-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn caminho_do_metadado_acompanha_o_arquivo() {
        let meta = meta_path_for(Path::new("src/main.rs"));
        assert_eq!(meta, PathBuf::from("src/main.rs.c2.json"));
        assert!(is_meta_path(&meta));
        assert!(!is_meta_path(Path::new("src/main.rs")));
        assert_eq!(source_path_for(&meta), PathBuf::from("src/main.rs"));
    }

    #[test]
    fn parse_schema_v1() {
        let raw = br#"{
            "schema_version": 1,
            "file": "src/a.rs",
            "hash": "sha256:abc",
            "size": 10,
            "mtime": 1700000000,
            "semantic": {"what": "w", "why": "y", "goal": "g", "summary": "s", "responsibility": "r"},
            "links": [{"to": "src/b.rs", "type": "imports"}]
        }"#;
        let meta = MetaFile::parse(raw).unwrap();
        assert_eq!(meta.schema_version, 1);
        assert_eq!(meta.semantic.what, "w");
        assert_eq!(meta.semantic.responsibility, "r");
        assert_eq!(meta.links.len(), 1);
        assert_eq!(meta.links[0].kind, "imports");
    }

    #[test]
    fn aceita_bom_utf8() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(br#"{"file":"a.rs","hash":"h","mtime":1}"#);
        let meta = MetaFile::parse(&bytes).unwrap();
        assert_eq!(meta.file, "a.rs");
    }

    #[test]
    fn when_do_prototipo_preserva_o_conteudo_em_what() {
        let legacy = br#"{
            "file": "src/a.sh",
            "hash": "abc",
            "mtime": 1,
            "semantic": {
                "why": "porque",
                "when": "usado no build noturno",
                "goal": "publicar",
                "summary": "resumo"
            }
        }"#;
        let meta = MetaFile::parse(legacy).unwrap();
        assert_eq!(meta.semantic.what, "usado no build noturno");
        assert_eq!(meta.semantic.why, "porque");
        assert_eq!(meta.semantic.goal, "publicar");
    }

    #[test]
    fn parse_metadado_do_prototipo_bash() {
        // formato do loop_do-GPT_SEMANTIC_GENERATOR.sh: sem schema_version,
        // sem what/responsibility/links, com `when` no lugar.
        let raw = br#"{
            "file": "./src/a.rs",
            "hash": "abc",
            "mtime": 1700000000,
            "semantic": {"why": "y", "when": "quando", "goal": "g", "summary": "s"}
        }"#;
        let meta = MetaFile::parse(raw).unwrap();
        assert_eq!(meta.schema_version, SCHEMA_VERSION);
        assert_eq!(meta.semantic.why, "y");
        assert_eq!(meta.semantic.goal, "g");
        // `when` do prototipo vira `what` (nao se perde na leitura).
        assert_eq!(meta.semantic.what, "quando");
        assert!(meta.links.is_empty());
    }

    #[test]
    fn parse_campos_soltos_na_raiz() {
        // formato que o c2_context.sh aceita como fallback.
        let raw = br#"{"file":"a.rs","hash":"h","what":"w","why":"y","goal":"g","summary":"s","dependencies":["b.rs"]}"#;
        let meta = MetaFile::parse(raw).unwrap();
        assert_eq!(meta.semantic.what, "w");
        assert_eq!(meta.links.len(), 1);
        assert_eq!(meta.links[0].to, "b.rs");
    }

    #[test]
    fn metadado_vazio_e_recusado() {
        let meta = MetaFile::new("a.rs", "sha256:x", 1, 1, Semantic::default(), vec![]);
        assert!(meta.validate().is_err());
    }

    #[test]
    fn escrita_atomica_grava_e_sobrescreve() {
        let base = dir("write");
        let path = base.join("a.rs.c2.json");
        let semantic = Semantic {
            what: "w".into(),
            ..Default::default()
        };
        let meta = MetaFile::new("a.rs", "sha256:1", 1, 1, semantic.clone(), vec![]);
        meta.write_atomic(&path).unwrap();
        assert_eq!(MetaFile::read(&path).unwrap(), meta);

        let meta2 = MetaFile::new("a.rs", "sha256:2", 2, 2, semantic, vec![]);
        meta2.write_atomic(&path).unwrap();
        assert_eq!(MetaFile::read(&path).unwrap().hash, "sha256:2");

        // nenhum temporario sobra no diretorio
        let leftovers: Vec<_> = fs::read_dir(&base)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with(".c2-tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temporarios: {leftovers:?}");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn json_invalido_vira_erro_de_metadado() {
        let base = dir("invalid");
        let path = base.join("bad.c2.json");
        fs::write(&path, b"{ nada").unwrap();
        let err = MetaFile::read(&path).unwrap_err();
        assert!(matches!(err, C2Error::InvalidMeta { .. }));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn frescor_compara_hash_e_tamanho() {
        let meta = MetaFile::new(
            "a.rs",
            "sha256:1",
            7,
            1,
            Semantic {
                what: "w".into(),
                ..Default::default()
            },
            vec![],
        );
        assert!(meta.is_fresh("sha256:1", 7));
        assert!(!meta.is_fresh("sha256:2", 7));
        assert!(!meta.is_fresh("sha256:1", 8));
    }
}
