//! Floresta teleologica: resumos de pasta/dominio e master do projeto.
//!
//! Tasks `c2-jgf.1`, `.2`, `.3`, `.4` e `.5`. A geracao e deterministica a
//! partir dos metadados, docs e agent_notes; nao inventa fatos sem evidencia.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cleaner;
use crate::config::Config;
use crate::error::{C2Error, IoContext, Result};
use crate::schema::{AgentNote, MetaFile};
use crate::workspace::Workspace;

const SCHEMA_VERSION: u32 = 1;
const MAX_BRIEF_CHARS: usize = 6000;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Teleology {
    pub what: String,
    pub why: String,
    pub goal: String,
    pub who: Vec<String>,
    pub success: Vec<String>,
    pub non_goals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceRef {
    pub path: String,
    pub hash: String,
    pub mtime: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSummary {
    pub schema_version: u32,
    pub path: String,
    pub teleology: Teleology,
    pub boundaries: Vec<String>,
    pub key_files: Vec<String>,
    pub depends_on: Vec<String>,
    pub sources: Vec<String>,
    pub subtree_hash: String,
    pub generated_at: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEntry {
    pub path: String,
    pub what: String,
    pub why: String,
    pub goal: String,
    pub boundaries: Vec<String>,
    pub key_files: Vec<String>,
    pub depends_on: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEntry {
    pub name: String,
    pub steps: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEntry {
    pub title: String,
    pub severity: String,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Divergence {
    pub kind: String,
    pub claim: String,
    pub docs_ref: String,
    pub code_ref: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Architecture {
    pub summary: String,
    pub layers: Vec<String>,
    pub entrypoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forest {
    pub schema_version: u32,
    pub generated_at: String,
    pub teleology: Teleology,
    pub architecture: Architecture,
    pub domains: Vec<DomainEntry>,
    pub flows: Vec<FlowEntry>,
    pub decisions: Vec<DecisionEntry>,
    pub risks: Vec<RiskEntry>,
    pub divergences: Vec<Divergence>,
    pub sources: Vec<SourceRef>,
    pub section_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BuildReport {
    pub folders: usize,
    pub folders_reused: usize,
    pub files: usize,
    pub notes: usize,
    pub docs: usize,
    pub forest_written: bool,
}

fn agora() -> String {
    crate::notes::agora_iso()
}

fn hash_text(texto: &str) -> String {
    let mut h = Sha256::new();
    h.update(texto.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

fn slug(path: &str) -> String {
    path.replace(['/', '\\'], "__")
}

pub fn folder_path(workspace: &Workspace, domain: &str) -> PathBuf {
    workspace
        .c2_dir()
        .join("folders")
        .join(format!("{}.json", slug(domain)))
}

pub fn forest_path(workspace: &Workspace) -> PathBuf {
    workspace.c2_dir().join("forest.json")
}

/// Arquivos de configuracao/lock e docs da raiz nao viram "dominios".
fn entra_na_floresta(file: &str) -> bool {
    let nome = file.rsplit('/').next().unwrap_or(file);
    if matches!(
        nome,
        "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | "Cargo.lock"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "bun.lockb"
            | "composer.lock"
            | "Pipfile.lock"
            | "poetry.lock"
            | "Gemfile.lock"
            | "flake.lock"
            | "go.sum"
    ) {
        return false;
    }
    // README/PRD/ADRs entram como fontes documentais, nao como pastas.
    !(!file.contains('/') && matches!(nome.rsplit('.').next(), Some("md" | "txt")))
}

/// Diretorio/dominio ao qual um arquivo pertence.
pub fn domain_for(file: &str) -> String {
    let partes: Vec<&str> = file.split('/').filter(|p| !p.is_empty()).collect();
    if partes.is_empty() {
        return "(raiz)".to_string();
    }
    if matches!(partes[0], "src" | "crates" | "apps") {
        if partes.len() >= 3 {
            return format!("{}/{}", partes[0], partes[1]);
        }
        return partes[0].to_string();
    }
    partes[0].to_string()
}

fn resumo_unico(valores: impl Iterator<Item = String>, limite: usize) -> String {
    let mut vistos = BTreeSet::new();
    let mut saida = Vec::new();
    for valor in valores {
        let valor = valor.trim().to_string();
        if valor.is_empty() || !vistos.insert(valor.clone()) {
            continue;
        }
        saida.push(valor);
        if saida.len() >= limite {
            break;
        }
    }
    saida.join(" ")
}

fn doms_links(meta: &MetaFile) -> BTreeSet<String> {
    meta.links
        .iter()
        .map(|link| domain_for(&link.to))
        .filter(|dom| dom != &domain_for(&meta.file) && dom != "(raiz)")
        .collect()
}

fn subtree_hash(metas: &[MetaFile]) -> String {
    let mut linhas: Vec<String> = metas
        .iter()
        .map(|meta| format!("{}:{}", meta.file, meta.hash))
        .collect();
    linhas.sort();
    hash_text(&linhas.join("\n"))
}

fn resumo_pasta(
    path: &str,
    metas: &[MetaFile],
    existente: Option<&FolderSummary>,
) -> FolderSummary {
    let hash = subtree_hash(metas);
    if let Some(anterior) = existente {
        if anterior.subtree_hash == hash {
            return anterior.clone();
        }
    }
    let what = resumo_unico(metas.iter().map(|m| m.semantic.what.clone()), 4);
    let why = resumo_unico(metas.iter().map(|m| m.semantic.why.clone()), 3);
    let goal = resumo_unico(metas.iter().map(|m| m.semantic.goal.clone()), 3);
    let mut boundaries = Vec::new();
    for meta in metas {
        if !meta.semantic.responsibility.trim().is_empty() {
            boundaries.push(meta.semantic.responsibility.clone());
        }
        if boundaries.len() >= 3 {
            break;
        }
    }
    let mut key_files: Vec<String> = metas
        .iter()
        .map(|meta| meta.file.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    key_files.truncate(8);
    let mut depends_on = BTreeSet::new();
    for meta in metas {
        depends_on.extend(doms_links(meta));
    }
    let mut sources: Vec<String> = metas
        .iter()
        .map(|meta| format!("{}.c2.json", meta.file))
        .collect();
    sources.sort();
    let confidence = if what.is_empty() { "low" } else { "medium" };
    FolderSummary {
        schema_version: SCHEMA_VERSION,
        path: path.to_string(),
        teleology: Teleology {
            what,
            why,
            goal,
            who: Vec::new(),
            success: Vec::new(),
            non_goals: Vec::new(),
        },
        boundaries,
        key_files,
        depends_on: depends_on.into_iter().collect(),
        sources,
        subtree_hash: hash,
        generated_at: agora(),
        confidence: confidence.to_string(),
    }
}

fn escrever_json<T: Serialize>(path: &Path, valor: &T) -> Result<()> {
    if let Some(pai) = path.parent() {
        fs::create_dir_all(pai).at(pai)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(valor)?).at(&tmp)?;
    fs::rename(&tmp, path).at(path)
}

fn ler_folder(path: &Path) -> Option<FolderSummary> {
    fs::read_to_string(path)
        .ok()
        .and_then(|texto| serde_json::from_str(&texto).ok())
}

/// Constroi folders + forest a partir do workspace.
pub fn build(workspace: &Workspace, _config: &Config) -> Result<BuildReport> {
    let mut por_dominio: BTreeMap<String, Vec<MetaFile>> = BTreeMap::new();
    let mut total_notas = 0usize;
    for meta_path in cleaner::metadata_files(workspace.root())? {
        let Ok(meta) = MetaFile::read(&meta_path) else {
            continue;
        };
        if !entra_na_floresta(&meta.file) {
            continue;
        }
        total_notas += meta.agent_notes.len();
        por_dominio
            .entry(domain_for(&meta.file))
            .or_default()
            .push(meta);
    }
    let mut report = BuildReport {
        files: por_dominio.values().map(Vec::len).sum(),
        notes: total_notas,
        ..BuildReport::default()
    };

    let mut folders = Vec::new();
    for (domain, mut metas) in por_dominio {
        metas.sort_by(|a, b| a.file.cmp(&b.file));
        let caminho = folder_path(workspace, &domain);
        let anterior = ler_folder(&caminho);
        let folder = resumo_pasta(&domain, &metas, anterior.as_ref());
        if anterior
            .as_ref()
            .map(|a| a.subtree_hash == folder.subtree_hash)
            .unwrap_or(false)
        {
            report.folders_reused += 1;
        } else {
            report.folders += 1;
        }
        escrever_json(&caminho, &folder)?;
        folders.push(folder);
    }

    let pasta_dir = workspace.c2_dir().join("folders");
    if pasta_dir.is_dir() {
        let esperados: BTreeSet<std::path::PathBuf> = folders
            .iter()
            .map(|f| folder_path(workspace, &f.path))
            .collect();
        if let Ok(arquivos) = fs::read_dir(&pasta_dir) {
            for arquivo in arquivos.flatten() {
                let path = arquivo.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json")
                    && !esperados.contains(&path)
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }

    let fontes = fontes_documentais(workspace)?;
    report.docs = fontes.len();
    let (teleologia_docs, doc_text) = teleologia_dos_docs(workspace, &fontes);

    let mut domains = Vec::new();
    for folder in &folders {
        domains.push(DomainEntry {
            path: folder.path.clone(),
            what: folder.teleology.what.clone(),
            why: folder.teleology.why.clone(),
            goal: folder.teleology.goal.clone(),
            boundaries: folder.boundaries.clone(),
            key_files: folder.key_files.clone(),
            depends_on: folder.depends_on.clone(),
            evidence: folder.sources.clone(),
        });
    }

    let mut decisions = Vec::new();
    let mut risks = Vec::new();
    coletar_decisoes_e_riscos(workspace, &mut decisions, &mut risks)?;
    for decisao in &mut decisions {
        if decisao.evidence.is_empty() && !doc_text.is_empty() {
            decisao.evidence.push("README.md".to_string());
        }
    }

    let mut section_hashes = BTreeMap::new();
    section_hashes.insert(
        "visao".to_string(),
        hash_text(&serde_json::to_string(&teleologia_docs)?),
    );
    section_hashes.insert(
        "arquitetura".to_string(),
        hash_text(&serde_json::to_string(&domains)?),
    );
    section_hashes.insert(
        "dominios".to_string(),
        hash_text(&serde_json::to_string(&domains)?),
    );
    section_hashes.insert(
        "decisoes".to_string(),
        hash_text(&serde_json::to_string(&decisions)?),
    );
    section_hashes.insert(
        "riscos".to_string(),
        hash_text(&serde_json::to_string(&risks)?),
    );

    let forest = Forest {
        schema_version: SCHEMA_VERSION,
        generated_at: agora(),
        teleology: teleologia_docs,
        architecture: Architecture {
            summary: format!("{} domínios mapeados", domains.len()),
            layers: domains.iter().map(|d| d.path.clone()).collect(),
            entrypoints: entrypoints(workspace),
        },
        domains,
        flows: vec![],
        decisions,
        risks,
        divergences: vec![],
        sources: fontes,
        section_hashes,
    };
    escrever_json(&forest_path(workspace), &forest)?;
    report.forest_written = true;
    Ok(report)
}

fn entrypoints(workspace: &Workspace) -> Vec<String> {
    [
        "src/main.rs",
        "src/lib.rs",
        "apps/desktop/src-tauri/src/main.rs",
    ]
    .iter()
    .filter(|rel| workspace.root().join(rel).is_file())
    .map(|rel| rel.to_string())
    .collect()
}

fn fontes_documentais(workspace: &Workspace) -> Result<Vec<SourceRef>> {
    let mut fontes = Vec::new();
    for rel in ["README.md", "PRD.md", "CLAUDE.md", "AGENTS.md"] {
        let path = workspace.root().join(rel);
        if path.is_file() {
            fontes.push(SourceRef {
                path: rel.to_string(),
                hash: crate::hash::sha256_file(&path)?,
                mtime: mtime(&path),
            });
        }
    }
    let adr = workspace.root().join("docs/adr");
    if adr.is_dir() {
        let mut arquivos = fs::read_dir(&adr)
            .at(&adr)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        arquivos.sort();
        for path in arquivos {
            let rel = path
                .strip_prefix(workspace.root())
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            fontes.push(SourceRef {
                path: rel,
                hash: crate::hash::sha256_file(&path)?,
                mtime: mtime(&path),
            });
        }
    }
    Ok(fontes)
}

fn mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn primeiro_titulo(texto: &str) -> Option<String> {
    texto.lines().find_map(|linha| {
        linha
            .trim()
            .strip_prefix("# ")
            .map(|titulo| titulo.trim().to_string())
    })
}

fn primeiro_paragrafo(texto: &str) -> Option<String> {
    let mut dentro = false;
    let mut linhas = Vec::new();
    for linha in texto.lines() {
        let l = linha.trim();
        if l.is_empty() {
            if dentro && !linhas.is_empty() {
                break;
            }
            continue;
        }
        if l.starts_with('#') || l.starts_with("```") || l.starts_with('<') {
            if dentro && !linhas.is_empty() {
                break;
            }
            continue;
        }
        dentro = true;
        linhas.push(l.to_string());
        if linhas.len() >= 3 {
            break;
        }
    }
    if linhas.is_empty() {
        None
    } else {
        Some(linhas.join(" "))
    }
}

fn teleologia_dos_docs(workspace: &Workspace, fontes: &[SourceRef]) -> (Teleology, String) {
    let mut texto = String::new();
    for fonte in fontes.iter().take(2) {
        if let Ok(conteudo) = fs::read_to_string(workspace.root().join(&fonte.path)) {
            texto.push_str(&conteudo);
            texto.push('\n');
        }
    }
    let nome = workspace
        .root()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "projeto".to_string());
    let teleologia = Teleology {
        what: primeiro_titulo(&texto).unwrap_or_else(|| nome.clone()),
        why: primeiro_paragrafo(&texto).unwrap_or_default(),
        goal: format!("manter {} compreensível para agentes e humanos", nome),
        who: vec![
            "desenvolvedores".to_string(),
            "agentes de código".to_string(),
        ],
        success: Vec::new(),
        non_goals: Vec::new(),
    };
    (teleologia, texto)
}

fn coletar_decisoes_e_riscos(
    workspace: &Workspace,
    decisions: &mut Vec<DecisionEntry>,
    risks: &mut Vec<RiskEntry>,
) -> Result<()> {
    let adr = workspace.root().join("docs/adr");
    if adr.is_dir() {
        let mut arquivos = fs::read_dir(&adr)
            .at(&adr)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        arquivos.sort();
        for (indice, path) in arquivos.into_iter().enumerate() {
            let texto = fs::read_to_string(&path).at(&path)?;
            let rel = path
                .strip_prefix(workspace.root())
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            decisions.push(DecisionEntry {
                id: format!("ADR-{:03}", indice + 1),
                title: primeiro_titulo(&texto).unwrap_or_else(|| rel.clone()),
                status: "accepted".to_string(),
                summary: primeiro_paragrafo(&texto).unwrap_or_default(),
                evidence: vec![rel],
            });
        }
    }
    for meta_path in cleaner::metadata_files(workspace.root())? {
        let Ok(meta) = MetaFile::read(&meta_path) else {
            continue;
        };
        for nota in meta.agent_notes {
            match nota.kind.as_str() {
                "teleology" | "invariant" => decisions.push(DecisionEntry {
                    id: hash_text(&format!("{}:{}", meta.file, nota.at)),
                    title: format!("{} em {}", nota.kind, meta.file),
                    status: "accepted".to_string(),
                    summary: nota.text.clone(),
                    evidence: vec![format!("{}#{}", meta.file, nota.at)],
                }),
                "pending" | "gotcha" | "correction" => risks.push(RiskEntry {
                    title: format!("{} em {}", nota.kind, meta.file),
                    severity: if nota.kind == "pending" {
                        "medium".to_string()
                    } else {
                        "low".to_string()
                    },
                    summary: nota.text.clone(),
                    evidence: vec![format!("{}#{}", meta.file, nota.at)],
                }),
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn load(workspace: &Workspace) -> Result<Forest> {
    let path = forest_path(workspace);
    let texto = fs::read_to_string(&path).at(&path)?;
    serde_json::from_str(&texto).map_err(C2Error::from)
}

pub fn load_folder(workspace: &Workspace, domain: &str) -> Result<FolderSummary> {
    let path = folder_path(workspace, domain);
    let texto = fs::read_to_string(&path).at(&path)?;
    serde_json::from_str(&texto).map_err(C2Error::from)
}

/// Briefing curto para onboarding/tarefa/agente.
pub fn brief(workspace: &Workspace, domain: Option<&str>, task: Option<&str>) -> Result<String> {
    let forest = load(workspace)?;
    let mut saida = String::new();
    saida.push_str(&format!("# {}\n", forest.teleology.what));
    if !forest.teleology.why.is_empty() {
        saida.push_str(&format!("{}\n", forest.teleology.why));
    }
    saida.push_str(&format!("objetivo: {}\n\n", forest.teleology.goal));

    let dominios: Vec<&DomainEntry> = match domain {
        Some(dom) => forest
            .domains
            .iter()
            .filter(|d| d.path == dom || d.path.starts_with(&format!("{dom}/")))
            .collect(),
        None => forest.domains.iter().collect(),
    };
    saida.push_str("## domínios\n");
    for dom in dominios.iter().take(8) {
        saida.push_str(&format!("- {}: {}\n", dom.path, dom.what));
    }

    if let Some(termo) = task {
        saida.push_str("\n## foco da tarefa\n");
        saida.push_str(&format!("buscar por: {termo}\n"));
    }

    let notas = notas_relevantes(workspace, domain)?;
    if !notas.is_empty() {
        saida.push_str("\n## memória relevante\n");
        for (arquivo, nota) in notas.iter().take(8) {
            saida.push_str(&format!("- [{}] {}: {}\n", nota.kind, arquivo, nota.text));
        }
    }

    let riscos: Vec<&RiskEntry> = forest
        .risks
        .iter()
        .filter(|r| {
            domain
                .map(|d| r.evidence.iter().any(|e| e.starts_with(d)))
                .unwrap_or(true)
        })
        .collect();
    if !riscos.is_empty() {
        saida.push_str("\n## riscos/gotchas\n");
        for risco in riscos.iter().take(6) {
            saida.push_str(&format!("- {}: {}\n", risco.title, risco.summary));
        }
    }
    if saida.chars().count() > MAX_BRIEF_CHARS {
        saida = saida.chars().take(MAX_BRIEF_CHARS).collect();
        saida.push_str("\n... (brief truncado)");
    }
    Ok(saida)
}

fn notas_relevantes(
    workspace: &Workspace,
    domain: Option<&str>,
) -> Result<Vec<(String, AgentNote)>> {
    let mut notas = Vec::new();
    for (file, lista) in crate::notes::list(workspace, None)? {
        if let Some(dom) = domain {
            if !file.starts_with(dom) {
                continue;
            }
        }
        for nota in lista {
            if matches!(nota.kind.as_str(), "gotcha" | "correction" | "invariant") {
                notas.push((file.clone(), nota));
            }
        }
    }
    Ok(notas)
}

/// Texto usado para o embedding de cada secao do forest.
pub fn section_text(forest: &Forest, section: &str) -> Option<String> {
    match section {
        "visao" => Some(format!(
            "{}\n{}\nobjetivo: {}",
            forest.teleology.what, forest.teleology.why, forest.teleology.goal
        )),
        "arquitetura" => Some(format!(
            "{}\n{}",
            forest.architecture.summary,
            forest.architecture.layers.join(", ")
        )),
        "dominios" => Some(
            forest
                .domains
                .iter()
                .map(|d| format!("{}: {} | {} | {}", d.path, d.what, d.why, d.goal))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "decisoes" => Some(
            forest
                .decisions
                .iter()
                .map(|d| format!("{} {}: {}", d.title, d.status, d.summary))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "riscos" => Some(
            forest
                .risks
                .iter()
                .map(|r| format!("{} ({}): {}", r.title, r.severity, r.summary))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// Gera vetores separados por secao. Requer runtime de embeddings.
pub fn embed_sections(workspace: &Workspace, config: &Config) -> Result<usize> {
    let cfg = &config.index.embeddings;
    if !cfg.enabled {
        return Ok(0);
    }
    crate::embedding_runtime::ensure(workspace, config)?;
    let forest = load(workspace)?;
    let cache = crate::embedding::EmbeddingCache::new(workspace, &cfg.model_id, cfg.dim);
    let mut gerados = 0usize;
    for secao in ["visao", "arquitetura", "dominios", "decisoes", "riscos"] {
        let Some(texto) = section_text(&forest, secao) else {
            continue;
        };
        let hash = forest
            .section_hashes
            .get(secao)
            .cloned()
            .unwrap_or_else(|| hash_text(&texto));
        let chave = format!("forest/{secao}");
        if cache.load(&chave, &hash).is_some() {
            continue;
        }
        let vetor = crate::embedding_runtime::embed(config, &texto)?;
        if vetor.len() == cfg.dim {
            cache
                .store(&chave, &hash, &vetor)
                .map_err(|e| C2Error::InvalidConfig(format!("cache forest: {e}")))?;
            gerados += 1;
        }
    }
    Ok(gerados)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Link, Semantic};

    fn tmp(tag: &str) -> (PathBuf, Workspace) {
        let dir = std::env::temp_dir().join(format!(
            "c2-forest-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join(".c2")).unwrap();
        (dir.clone(), Workspace::at(dir))
    }

    fn meta(rel: &str, hash: &str, note_kind: Option<&str>) -> MetaFile {
        let mut m = MetaFile::new(
            rel,
            hash,
            1,
            0,
            Semantic {
                what: "pagamento".to_string(),
                why: "cobrar".to_string(),
                goal: "confirmar".to_string(),
                summary: "resumo".to_string(),
                responsibility: "dono".to_string(),
            },
            vec![Link {
                to: "src/pedido.rs".to_string(),
                kind: "imports".to_string(),
            }],
        );
        if let Some(kind) = note_kind {
            m.agent_notes.push(AgentNote {
                by: "agent".to_string(),
                model: None,
                task: None,
                at: "2026-11-10T10:00:00Z".to_string(),
                hash: Some(hash.to_string()),
                kind: kind.to_string(),
                text: "cuidado com retry".to_string(),
            });
        }
        m
    }

    #[test]
    fn dominio_de_caminhos() {
        assert_eq!(domain_for("src/pagamento/servico.rs"), "src/pagamento");
        assert_eq!(domain_for("apps/desktop/src/main.ts"), "apps/desktop");
        assert_eq!(domain_for("README.md"), "README.md");
    }

    #[test]
    fn build_cria_folders_e_forest_com_notes() {
        let (dir, workspace) = tmp("build");
        workspace.init(false).unwrap();
        let a = meta("src/pagamento/a.rs", "sha256:a", Some("gotcha"));
        let b = meta("src/pedido/b.rs", "sha256:b", None);
        escribir_meta(&workspace, &a);
        escribir_meta(&workspace, &b);
        let report = build(&workspace, &Config::default()).unwrap();
        assert!(report.forest_written);
        assert!(forest_path(&workspace).is_file());
        assert_eq!(report.folders, 2);
        let forest = load(&workspace).unwrap();
        assert_eq!(forest.domains.len(), 2);
        assert!(!forest.risks.is_empty());
        assert!(report.notes >= 1);
        let texto = brief(&workspace, Some("src/pagamento"), None).unwrap();
        assert!(texto.contains("pagamento"));
        fs::remove_dir_all(dir).ok();
    }

    fn escribir_meta(workspace: &Workspace, meta: &MetaFile) {
        let path = crate::schema::meta_path_for(&workspace.root().join(&meta.file));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        meta.write_atomic(&path).unwrap();
    }

    #[test]
    fn folder_reaproveita_hash() {
        let (dir, workspace) = tmp("reuse");
        workspace.init(false).unwrap();
        escribir_meta(&workspace, &meta("src/a.rs", "sha256:a", None));
        let r1 = build(&workspace, &Config::default()).unwrap();
        let r2 = build(&workspace, &Config::default()).unwrap();
        assert_eq!(r1.folders, 1);
        assert_eq!(r2.folders, 0);
        assert_eq!(r2.folders_reused, 1);
        fs::remove_dir_all(dir).ok();
    }
}
