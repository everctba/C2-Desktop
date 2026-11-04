//! Semantic Grep: busca hibrida (lexical + metadados) sem embeddings.
//!
//! Camadas (bead `c2-e8`):
//! 1. lexical: nome do arquivo, caminho e simbolos extraidos por heuristica
//!    (fn/def/function/class/struct...), sem parser;
//! 2. semantica: `what`, `why`, `goal`, `summary`, `responsibility` do
//!    metadado, com normalizacao, remocao de plural e mapa de sinonimos;
//! 3. explicacao: cada resultado diz qual campo e qual termo pontuaram
//!    (principio "evidencia antes de inferencia");
//! 4. trecho: linha onde o termo aparece, para o agente nao ler o arquivo todo.
//!
//! Sem embeddings, sem banco vetorial: casa com a decisao `c2-dec6`.

use std::collections::{BTreeMap, HashMap};
use std::fs;

use serde::Serialize;

use crate::config::Config;
use crate::error::Result;
use crate::schema::MetaFile;
use crate::workspace::Workspace;

/// Peso de um casamento no nome/caminho do arquivo.
const W_PATH: f32 = 3.0;
/// Peso de um casamento em um simbolo do codigo.
const W_SYMBOL: f32 = 2.0;
/// Peso de um casamento nos campos semanticos.
const W_SEMANTIC: f32 = 1.5;
/// Fator aplicado quando o casamento e parcial (substring).
const PARTIAL: f32 = 0.4;

/// Opcoes da busca.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub include_snippets: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 5,
            include_snippets: true,
        }
    }
}

/// Por que um arquivo apareceu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    /// `path`, `symbol` ou `semantic.<campo>`.
    pub field: String,
    /// Termos (ja normalizados) que casaram.
    pub terms: Vec<String>,
}

/// Trecho relevante no arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snippet {
    /// Primeira linha (1-based).
    pub line_start: usize,
    /// Ultima linha (1-based).
    pub line_end: usize,
    pub text: String,
}

/// Resultado da busca.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    /// Caminho relativo com `/`.
    pub file: String,
    pub score: f32,
    pub path_score: f32,
    pub symbol_score: f32,
    pub semantic_score: f32,
    pub evidence: Vec<Evidence>,
    pub summary: String,
    pub snippet: Option<Snippet>,
}

impl Hit {
    /// Motivo legivel, para a CLI mostrar sem `--explain`.
    pub fn reason(&self) -> String {
        self.evidence
            .iter()
            .map(|e| {
                if e.field.starts_with("semantic.") {
                    format!(
                        "{}: {}",
                        e.field.trim_start_matches("semantic."),
                        e.terms.join(",")
                    )
                } else {
                    format!("{}: {}", e.field, e.terms.join(","))
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Executa a busca sobre os metadados do workspace.
pub fn search(
    workspace: &Workspace,
    config: &Config,
    query: &str,
    options: &SearchOptions,
) -> Result<Vec<Hit>> {
    let expansions = build_expansions(config);
    let query_terms = expand(&tokenize(query), &expansions);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();
    let mut symbol_cache: HashMap<String, Vec<String>> = HashMap::new();

    for meta_path in crate::cleaner::metadata_files(workspace.root())? {
        let Ok(meta) = MetaFile::read(&meta_path) else {
            continue;
        };
        let rel = meta.file.clone();
        if rel.is_empty() {
            continue;
        }

        let path_tokens = tokenize(&rel);
        let semantic_fields = semantic_tokens(&meta);
        let symbols = symbols_for(workspace, &rel, &mut symbol_cache);

        let mut path_score = 0.0;
        let mut symbol_score = 0.0;
        let mut semantic_score = 0.0;
        let mut evidence: Vec<Evidence> = Vec::new();

        for term in &query_terms {
            let mut term_hit = false;

            if let Some(matched) = match_terms(term, &path_tokens) {
                path_score += W_PATH * matched;
                term_hit = true;
                push_evidence(&mut evidence, "path", term);
            }
            if let Some(matched) = match_terms(term, &symbols) {
                symbol_score += W_SYMBOL * matched;
                term_hit = true;
                push_evidence(&mut evidence, "symbol", term);
            }
            for (field, tokens) in &semantic_fields {
                if let Some(matched) = match_terms(term, tokens) {
                    semantic_score += W_SEMANTIC * matched;
                    term_hit = true;
                    push_evidence(&mut evidence, &format!("semantic.{field}"), term);
                }
            }

            if !term_hit {
                // Ultimo recurso: substring no resumo, para consulta com radical.
                if semantic_fields
                    .values()
                    .any(|tokens| tokens.iter().any(|t| t.contains(term.as_str())))
                {
                    semantic_score += W_SEMANTIC * PARTIAL;
                    push_evidence(&mut evidence, "semantic.summary", term);
                }
            }
        }

        let score = path_score + symbol_score + semantic_score;
        if score <= 0.0 {
            continue;
        }

        let snippet = if options.include_snippets {
            find_snippet(workspace, &rel, &query_terms)
        } else {
            None
        };

        hits.push(Hit {
            file: rel,
            score,
            path_score,
            symbol_score,
            semantic_score,
            evidence,
            summary: meta.semantic.summary.clone(),
            snippet,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file))
    });
    hits.truncate(options.limit.max(1));
    Ok(hits)
}

fn push_evidence(evidence: &mut Vec<Evidence>, field: &str, term: &str) {
    if let Some(existing) = evidence.iter_mut().find(|e| e.field == field) {
        if !existing.terms.iter().any(|t| t == term) {
            existing.terms.push(term.to_string());
        }
        return;
    }
    evidence.push(Evidence {
        field: field.to_string(),
        terms: vec![term.to_string()],
    });
}

/// Fator de casamento: 1.0 para igual, `PARTIAL` para substring, 0 para nada.
fn match_terms(term: &str, tokens: &[String]) -> Option<f32> {
    if tokens.iter().any(|t| t == term) {
        return Some(1.0);
    }
    if tokens
        .iter()
        .any(|t| t.starts_with(term) || term.starts_with(t.as_str()))
    {
        return Some(PARTIAL);
    }
    None
}

fn semantic_tokens(meta: &MetaFile) -> BTreeMap<&'static str, Vec<String>> {
    let mut fields = BTreeMap::new();
    fields.insert("what", tokenize(&meta.semantic.what));
    fields.insert("why", tokenize(&meta.semantic.why));
    fields.insert("goal", tokenize(&meta.semantic.goal));
    fields.insert("summary", tokenize(&meta.semantic.summary));
    fields.insert("responsibility", tokenize(&meta.semantic.responsibility));
    fields
}

/// Simbolos do arquivo, extraidos por heuristica de linha (sem parser).
fn symbols_for(
    workspace: &Workspace,
    rel: &str,
    cache: &mut HashMap<String, Vec<String>>,
) -> Vec<String> {
    if let Some(cached) = cache.get(rel) {
        return cached.clone();
    }
    let path = workspace
        .root()
        .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let symbols = fs::read_to_string(&path)
        .map(|content| extract_symbols(&content))
        .unwrap_or_default();
    cache.insert(rel.to_string(), symbols.clone());
    symbols
}

const SYMBOL_KEYWORDS: &[&str] = &[
    "fn",
    "def",
    "function",
    "class",
    "struct",
    "enum",
    "trait",
    "impl",
    "interface",
    "type",
    "const",
    "static",
    "pub",
    "async",
    "let",
    "var",
    "module",
    "defmodule",
    "func",
    "sub",
];

/// Extrai identificadores declarados (heuristica por palavra-chave).
pub fn extract_symbols(content: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in content.lines() {
        let tokens: Vec<&str> = line
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
            .filter(|t| !t.is_empty())
            .collect();
        for pair in tokens.windows(2) {
            if SYMBOL_KEYWORDS.contains(&pair[0]) {
                let name = pair[1].trim_matches(':');
                if name.len() >= 2 && !SYMBOL_KEYWORDS.contains(&name) {
                    symbols.push(name.to_ascii_lowercase());
                }
            }
        }
    }
    symbols.sort();
    symbols.dedup();
    symbols
}

/// Primeiro trecho do arquivo onde algum termo da consulta aparece.
fn find_snippet(workspace: &Workspace, rel: &str, terms: &[String]) -> Option<Snippet> {
    let path = workspace
        .root()
        .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let content = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    for (index, line) in lines.iter().enumerate() {
        let normalized = normalize(line);
        if terms.iter().any(|term| normalized.contains(term.as_str())) {
            let start = index.saturating_sub(1);
            let end = (index + 2).min(lines.len().saturating_sub(1));
            let text = lines[start..=end].join("\n");
            return Some(Snippet {
                line_start: start + 1,
                line_end: end + 1,
                text,
            });
        }
    }
    None
}

/// Minusculas e sem acentos.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let mapped = match ch {
            '\u{e1}' | '\u{e0}' | '\u{e2}' | '\u{e3}' | '\u{e4}' | '\u{e5}' | '\u{c1}'
            | '\u{c0}' | '\u{c2}' | '\u{c3}' | '\u{c4}' | '\u{c5}' => 'a',
            '\u{e9}' | '\u{e8}' | '\u{ea}' | '\u{eb}' | '\u{c9}' | '\u{c8}' | '\u{ca}'
            | '\u{cb}' => 'e',
            '\u{ed}' | '\u{ec}' | '\u{ee}' | '\u{ef}' | '\u{cd}' | '\u{cc}' | '\u{ce}'
            | '\u{cf}' => 'i',
            '\u{f3}' | '\u{f2}' | '\u{f4}' | '\u{f5}' | '\u{f6}' | '\u{d3}' | '\u{d2}'
            | '\u{d4}' | '\u{d5}' | '\u{d6}' => 'o',
            '\u{fa}' | '\u{f9}' | '\u{fb}' | '\u{fc}' | '\u{da}' | '\u{d9}' | '\u{db}'
            | '\u{dc}' => 'u',
            '\u{e7}' | '\u{c7}' => 'c',
            '\u{f1}' | '\u{d1}' => 'n',
            other => other.to_ascii_lowercase(),
        };
        out.push(mapped);
    }
    out
}

const STOPWORDS: &[&str] = &[
    "de", "da", "do", "das", "dos", "que", "para", "por", "com", "uma", "um", "os", "as", "no",
    "na", "nos", "nas", "em", "e", "o", "a", "ao", "aos", "se", "su", "entre", "sobre", "onde",
    "como", "quais", "qual", "the", "and", "for", "with", "that", "this", "from", "where", "what",
    "which", "is", "are", "to", "of", "in", "on", "an", "it", "be",
];

/// Tokeniza para busca: normaliza, divide, remove curtas e stopwords, tira plural.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = normalize(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .filter(|t| !STOPWORDS.contains(t))
        .map(stem)
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens
}

fn stem(token: &str) -> String {
    let len = token.len();
    // Plural portugues: sessoes -> sessao, papeis -> papel, fins -> fim.
    if len > 4 {
        if let Some(base) = token.strip_suffix("oes") {
            return format!("{base}ao");
        }
        if let Some(base) = token.strip_suffix("aes") {
            return format!("{base}ao");
        }
        if let Some(base) = token.strip_suffix("eis") {
            return format!("{base}el");
        }
        if let Some(base) = token.strip_suffix("ais") {
            return format!("{base}al");
        }
        if token.ends_with('s') {
            return token[..len - 1].to_string();
        }
    }
    token.to_string()
}

/// Mapa termo -> termos equivalentes, a partir de `search.synonyms`.
pub fn build_expansions(config: &Config) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for entry in &config.search.synonyms {
        let Some((head, tail)) = entry.split_once('=') else {
            continue;
        };
        let head_tokens = tokenize(head);
        let tail_tokens: Vec<String> = tail.split(',').flat_map(tokenize).collect();
        for head in &head_tokens {
            map.entry(head.clone())
                .or_default()
                .extend(tail_tokens.clone());
        }
        for extra in &tail_tokens {
            map.entry(extra.clone())
                .or_default()
                .extend(head_tokens.clone());
        }
    }
    map
}

fn expand(terms: &[String], expansions: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut out = terms.to_vec();
    for term in terms {
        if let Some(extra) = expansions.get(term) {
            out.extend(extra.iter().cloned());
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{MetaFile, Semantic};
    use crate::workspace::Workspace;
    use std::path::Path;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("c2-grep-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn add_file(base: &Path, rel: &str, content: &str, semantic: Semantic) {
        let path = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        let meta = MetaFile::new(
            rel,
            crate::hash::sha256_bytes(content.as_bytes()),
            content.len() as u64,
            1,
            semantic,
            vec![],
        );
        meta.write_atomic(&crate::schema::meta_path_for(&path))
            .unwrap();
    }

    fn semantic(what: &str, why: &str, summary: &str) -> Semantic {
        Semantic {
            what: what.into(),
            why: why.into(),
            goal: "garantir acesso".into(),
            summary: summary.into(),
            responsibility: "dono do comportamento".into(),
        }
    }

    fn corpus() -> (PathBuf, Workspace, Config) {
        let base = tmp("corpus");
        add_file(
            &base,
            "src/auth/session.rs",
            "pub fn validate_session(token: &str) {}\n",
            semantic(
                "gerenciamento de sessao",
                "validar token de autenticacao",
                "sessao e token",
            ),
        );
        add_file(
            &base,
            "src/billing/invoice.rs",
            "pub fn total() -> u32 { 0 }\n",
            semantic("calculo de fatura", "emitir cobranca", "fatura do cliente"),
        );
        add_file(
            &base,
            "src/ui/theme.rs",
            "pub fn color() {}\n",
            semantic("tema visual", "definir cores", "aparencia"),
        );
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        let config = workspace.load_config().unwrap();
        (base, workspace, config)
    }

    #[test]
    fn encontra_por_significado() {
        let (base, workspace, config) = corpus();
        let hits = search(
            &workspace,
            &config,
            "onde valida autenticacao de usuario",
            &SearchOptions::default(),
        )
        .unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].file, "src/auth/session.rs");
        assert!(hits[0].score > 0.0);
        assert!(hits[0].evidence.iter().any(|e| e.field == "semantic.why"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn sinonimos_ampliam_a_busca() {
        let (base, workspace, config) = corpus();
        let hits = search(&workspace, &config, "login", &SearchOptions::default()).unwrap();
        assert_eq!(hits[0].file, "src/auth/session.rs");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn encontra_por_nome_de_funcao() {
        let (base, workspace, config) = corpus();
        let hits = search(
            &workspace,
            &config,
            "validate_session",
            &SearchOptions::default(),
        )
        .unwrap();
        assert_eq!(hits[0].file, "src/auth/session.rs");
        assert!(hits[0].symbol_score > 0.0);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn encontra_por_nome_de_arquivo() {
        let (base, workspace, config) = corpus();
        let hits = search(&workspace, &config, "invoice", &SearchOptions::default()).unwrap();
        assert_eq!(hits[0].file, "src/billing/invoice.rs");
        assert!(hits[0].path_score > 0.0);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn respeita_limite_e_ordena_por_score() {
        let (base, workspace, config) = corpus();
        let hits = search(
            &workspace,
            &config,
            "sessao token fatura",
            &SearchOptions {
                limit: 1,
                include_snippets: false,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn devolve_trecho_com_linhas() {
        let (base, workspace, config) = corpus();
        let hits = search(
            &workspace,
            &config,
            "validate_session",
            &SearchOptions::default(),
        )
        .unwrap();
        let snippet = hits[0].snippet.as_ref().expect("trecho");
        assert_eq!(snippet.line_start, 1);
        assert!(snippet.text.contains("validate_session"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn consulta_sem_termo_util_devolve_vazio() {
        let (base, workspace, config) = corpus();
        let hits = search(&workspace, &config, "de o a", &SearchOptions::default()).unwrap();
        assert!(hits.is_empty());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn normaliza_acentos_e_plural() {
        assert_eq!(normalize("Autentica\u{e7}\u{e3}o"), "autenticacao");
        assert!(tokenize("sess\u{f5}es").contains(&"sessao".to_string()));
        assert_eq!(
            extract_symbols("pub fn valida_token() {}"),
            vec!["valida_token"]
        );
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::schema::{MetaFile, Semantic};
    use crate::workspace::Workspace;
    use std::time::Instant;

    #[test]
    #[ignore = "benchmark manual (c2-e8t6)"]
    fn bench_busca_corpus_sintetico() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("c2-bench-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&base).unwrap();

        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        let config = workspace.load_config().unwrap();

        for i in 0..500usize {
            let rel = format!("src/mod{}/arquivo_{i}.rs", i % 20);
            let path = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let content = format!("pub fn funcao_{i}() {{}}\n");
            fs::write(&path, content.as_bytes()).unwrap();
            let semantic = Semantic {
                what: format!("modulo {i} de autenticacao e sessao"),
                why: "validar acesso do usuario".to_string(),
                goal: "seguranca".to_string(),
                summary: format!("responsabilidade do modulo {i}"),
                responsibility: "dono do modulo".to_string(),
            };
            let meta = MetaFile::new(
                &rel,
                crate::hash::sha256_bytes(content.as_bytes()),
                content.len() as u64,
                1,
                semantic,
                vec![],
            );
            meta.write_atomic(&crate::schema::meta_path_for(&path))
                .unwrap();
        }

        let queries = [
            "onde valida autenticacao",
            "modulo de sessao do usuario",
            "calculo de fatura",
            "configuracao do modelo",
            "funcao_42",
            "seguranca de acesso",
        ];

        let mut times = Vec::new();
        for query in queries {
            let started = Instant::now();
            let hits = search(&workspace, &config, query, &SearchOptions::default()).unwrap();
            let ms = started.elapsed().as_millis();
            println!("{query} -> {} resultados em {ms} ms", hits.len());
            times.push(ms);
        }
        times.sort();
        let p50 = times[times.len() / 2];
        let p95 = times[times.len() * 95 / 100];
        println!("500 arquivos: p50={p50} ms p95={p95} ms");

        fs::remove_dir_all(&base).ok();
    }
}
