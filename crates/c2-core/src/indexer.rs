//! Indexador: decide o que fazer com cada arquivo e executa.
//!
//! Regras (beads `c2-e3`, `c2-e5`):
//! - hash sempre recalculado para decidir (SHA-256 e barato; mtime em segundos
//!   nao e confiavel para detectar edicao dentro do mesmo segundo);
//! - `full` forca reindexacao mesmo com hash igual;
//! - arquivo que muda durante a chamada do modelo nao e gravado: fica para o
//!   proximo ciclo (`c2-e3t5`);
//! - falha do modelo nunca grava metadado vazio.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::config::Config;
use crate::discover;
use crate::error::{C2Error, IoContext, Result};
use crate::hash;
use crate::links;
use crate::llm::SemanticProvider;
use crate::schema::{meta_path_for, source_path_for, MetaFile};
use crate::security::{self, SecretPolicy};
use crate::workspace::Workspace;

/// O que fazer com um arquivo neste ciclo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Decision {
    New,
    Changed,
    Unchanged,
    Renamed,
    Removed,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Decision::New => "novo",
            Decision::Changed => "alterado",
            Decision::Unchanged => "inalterado",
            Decision::Renamed => "renomeado",
            Decision::Removed => "removido",
        }
    }
}

/// Item do plano de indexacao.
#[derive(Debug, Clone)]
pub struct PlanItem {
    pub rel: String,
    pub path: PathBuf,
    pub meta_path: PathBuf,
    pub decision: Decision,
    pub size: u64,
    pub mtime: i64,
    pub hash: Option<String>,
    /// Metadado ja existente, quando houver.
    pub existing: Option<MetaFile>,
    /// O hash veio do cache (tamanho e mtime batem; nao recalculado).
    pub hash_from_cache: bool,
    /// Origem de uma renomeacao detectada por hash.
    pub renamed_from: Option<RenamedFrom>,
}

/// Metadado antigo de um arquivo renomeado.
#[derive(Debug, Clone)]
pub struct RenamedFrom {
    pub meta_path: PathBuf,
    pub old_rel: String,
}

/// Opcoes de um ciclo.
#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub full: bool,
    pub limit: Option<usize>,
    pub only_path: Option<String>,
    pub dry_run: bool,
}

/// Progresso emitido por arquivo.
#[derive(Debug, Clone)]
pub struct Progress {
    pub rel: String,
    pub decision: Decision,
    pub index: usize,
    pub total: usize,
}

/// Resultado de um ciclo.
#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexReport {
    pub indexed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub errors: usize,
    /// Arquivos que mudaram durante a analise e ficam para o proximo ciclo.
    pub deferred: usize,
    /// Metadados antigos que receberam links sem chamar o modelo.
    pub relinked: usize,
    /// Arquivos renomeados reaproveitando metadado (sem chamar o modelo).
    pub renamed: usize,
    /// Hashes reaproveitados do cache (tamanho+mtime iguais).
    pub hash_cache_hits: usize,
    /// Arquivos pulados por conter segredo (politica `skip`).
    pub blocked: usize,
    /// Arquivos indexados com conteudo mascarado (politica `mask`).
    pub masked: usize,
    /// Bytes de conteudo enviados ao modelo neste ciclo.
    pub bytes_sent: u64,
    pub calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub latency_ms: u64,
    pub last_error: Option<String>,
}

impl IndexReport {
    pub fn total(&self) -> usize {
        self.indexed
            + self.unchanged
            + self.removed
            + self.errors
            + self.deferred
            + self.relinked
            + self.renamed
            + self.blocked
            + self.masked
    }
}

/// Monta o plano: descobre arquivos, compara com os metadados, detecta
/// renomeacoes por hash e lista orfaos.
pub fn plan(
    workspace: &Workspace,
    config: &Config,
    full: bool,
    only_path: Option<&str>,
) -> Result<Vec<PlanItem>> {
    let root = workspace.root();
    let discovery = discover::discover(root, config)?;
    let cache = crate::hash::HashCache::load(&workspace.hash_cache_path());
    let mut items = Vec::new();

    for entry in discovery.files {
        if let Some(prefix) = only_path {
            if !entry.rel.starts_with(prefix) {
                continue;
            }
        }

        let meta_path = meta_path_for(&entry.path);
        let existing = if meta_path.is_file() {
            MetaFile::read(&meta_path).ok()
        } else {
            None
        };

        let (hash_atual, hash_from_cache) = fingerprint(&entry, &cache, full)?;
        let decision = match &existing {
            None => Decision::New,
            Some(meta) => {
                if !full && meta.hash == hash_atual {
                    Decision::Unchanged
                } else {
                    Decision::Changed
                }
            }
        };

        items.push(PlanItem {
            rel: entry.rel,
            path: entry.path,
            meta_path,
            decision,
            size: entry.size,
            mtime: entry.mtime,
            hash: Some(hash_atual),
            existing,
            hash_from_cache,
            renamed_from: None,
        });
    }

    // Orfaos: metadado cujo arquivo-fonte nao existe mais.
    let mut vistos: BTreeSet<PathBuf> = items.iter().map(|item| item.meta_path.clone()).collect();
    let mut orfaos: Vec<PlanItem> = Vec::new();
    for meta_path in crate::cleaner::metadata_files(root)? {
        if !vistos.insert(meta_path.clone()) {
            continue;
        }
        let source = source_path_for(&meta_path);
        if source.exists() {
            continue;
        }
        let rel = discover::relative_unix(root, &source);
        if let Some(prefix) = only_path {
            if !rel.starts_with(prefix) {
                continue;
            }
        }
        orfaos.push(PlanItem {
            rel,
            path: source,
            meta_path,
            decision: Decision::Removed,
            size: 0,
            mtime: 0,
            hash: None,
            existing: None,
            hash_from_cache: false,
            renamed_from: None,
        });
    }

    // Renomeacao: arquivo novo cujo hash casa com o de um orfao (mesmo conteudo).
    for item in items
        .iter_mut()
        .filter(|item| item.decision == Decision::New)
    {
        let Some(hash_novo) = item.hash.clone() else {
            continue;
        };
        let Some(posicao) = orfaos.iter().position(|orfao| {
            MetaFile::read(&orfao.meta_path)
                .map(|meta| meta.hash == hash_novo)
                .unwrap_or(false)
        }) else {
            continue;
        };

        let orfao = orfaos.remove(posicao);
        let existente = MetaFile::read(&orfao.meta_path).ok();
        item.decision = Decision::Renamed;
        item.existing = existente;
        item.renamed_from = Some(RenamedFrom {
            meta_path: orfao.meta_path,
            old_rel: orfao.rel,
        });
    }

    items.extend(orfaos);
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(items)
}

/// Hash do arquivo: reaproveita o cache quando (tamanho, mtime) batem e
/// `full` nao foi pedido; caso contrario le e calcula SHA-256.
fn fingerprint(
    entry: &discover::FileEntry,
    cache: &crate::hash::HashCache,
    full: bool,
) -> Result<(String, bool)> {
    if !full {
        if let Some(hash) = cache.get(&entry.rel, entry.size, entry.mtime) {
            return Ok((hash, true));
        }
    }
    Ok((hash::sha256_file(&entry.path)?, false))
}

/// Executa um ciclo de indexacao.
pub fn run<P: SemanticProvider>(
    workspace: &Workspace,
    config: &Config,
    provider: &P,
    options: &IndexOptions,
    mut on_progress: impl FnMut(&Progress),
) -> Result<IndexReport> {
    let items = plan(
        workspace,
        config,
        options.full,
        options.only_path.as_deref(),
    )?;
    let total = items.len();
    let hits_de_cache = items.iter().filter(|item| item.hash_from_cache).count();
    let mut report = IndexReport {
        hash_cache_hits: hits_de_cache,
        ..IndexReport::default()
    };
    let mut processed = 0usize;

    for (position, item) in items.iter().enumerate() {
        if let Some(limit) = options.limit {
            if processed >= limit {
                break;
            }
        }

        on_progress(&Progress {
            rel: item.rel.clone(),
            decision: item.decision,
            index: position + 1,
            total,
        });

        match item.decision {
            Decision::Unchanged => {
                report.unchanged += 1;
                if !options.dry_run {
                    if let Some(meta) = &item.existing {
                        // Metadado antigo (ou de versao anterior do extrator) sem
                        // arestas: completa o grafo sem chamar o modelo.
                        if meta.links.is_empty() {
                            let extracted = extract_links(workspace, config, item)?;
                            if !extracted.is_empty() {
                                let updated = MetaFile::new(
                                    meta.file.clone(),
                                    meta.hash.clone(),
                                    meta.size,
                                    meta.mtime,
                                    meta.semantic.clone(),
                                    extracted,
                                );
                                updated.write_atomic(&item.meta_path)?;
                                report.relinked += 1;
                            }
                        }
                    }
                }
            }
            Decision::Removed => {
                if options.dry_run {
                    report.removed += 1;
                } else {
                    match fs::remove_file(&item.meta_path).at(&item.meta_path) {
                        Ok(()) => report.removed += 1,
                        Err(err) => record_error(&mut report, err),
                    }
                }
            }
            Decision::Renamed => {
                processed += 1;
                if options.dry_run {
                    report.renamed += 1;
                    continue;
                }
                let (Some(origem), Some(meta)) = (&item.renamed_from, &item.existing) else {
                    continue;
                };
                let hash = item.hash.clone().unwrap_or_else(|| meta.hash.clone());
                let atualizado = MetaFile::new(
                    item.rel.clone(),
                    hash,
                    item.size,
                    item.mtime,
                    meta.semantic.clone(),
                    meta.links.clone(),
                );
                match atualizado.write_atomic(&item.meta_path) {
                    Ok(()) => {
                        let _ = fs::remove_file(&origem.meta_path);
                        match relink_references(workspace, &origem.old_rel, &item.rel) {
                            Ok(quantidade) => report.relinked += quantidade,
                            Err(err) => record_error(&mut report, err),
                        }
                        report.renamed += 1;
                    }
                    Err(err) => record_error(&mut report, err),
                }
            }
            Decision::New | Decision::Changed => {
                processed += 1;
                if options.dry_run {
                    continue;
                }
                // Teto de conteudo enviado ao modelo por ciclo (c2-e9t4).
                let budget = config.index.max_total_bytes_per_run;
                if budget > 0 && report.bytes_sent + item.size > budget {
                    report.deferred += 1;
                    report.last_error = Some(format!(
                        "limite de {} bytes por ciclo atingido; `{}` fica para o proximo",
                        budget, item.rel
                    ));
                    continue;
                }
                if let Err(err) = index_one(workspace, config, provider, item, &mut report) {
                    record_error(&mut report, err);
                }
            }
        }
    }

    // Persiste o cache apenas com os arquivos atuais (orfaos saem).
    if !options.dry_run {
        let mut atual = crate::hash::HashCache::default();
        for item in &items {
            if item.decision == Decision::Removed {
                continue;
            }
            let hash = item
                .hash
                .clone()
                .or_else(|| item.existing.as_ref().map(|meta| meta.hash.clone()));
            if let Some(hash) = hash {
                atual.insert(&item.rel, item.size, item.mtime, hash);
            }
        }
        atual.save(&workspace.hash_cache_path())?;
    }

    Ok(report)
}

fn index_one<P: SemanticProvider>(
    workspace: &Workspace,
    config: &Config,
    provider: &P,
    item: &PlanItem,
    report: &mut IndexReport,
) -> Result<()> {
    let metadata = fs::metadata(&item.path).at(&item.path)?;
    if metadata.len() > config.index.max_file_bytes {
        return Ok(());
    }

    let bytes = fs::read(&item.path).at(&item.path)?;
    let content = String::from_utf8(bytes)
        .map_err(|_| C2Error::InvalidConfig(format!("arquivo nao e UTF-8 valido: {}", item.rel)))?;
    if content.trim().is_empty() {
        return Ok(());
    }

    let hash_before = match &item.hash {
        Some(hash) => hash.clone(),
        None => hash::sha256_file(&item.path)?,
    };

    // Segredos nunca vao ao modelo (c2-e9t2). O hash continua sendo do conteudo
    // original: mascarar nao muda a identidade do arquivo.
    let allowlisted = config
        .index
        .secret_allow
        .iter()
        .any(|pattern| item.rel == *pattern || item.rel.starts_with(pattern.as_str()));

    let prompt_content = match config.index.secret_policy {
        _ if allowlisted => content.clone(),
        SecretPolicy::Skip => {
            if let Some(kind) = security::detect(&content) {
                report.blocked += 1;
                report.last_error = Some(format!(
                    "segredo ({}) em {}: arquivo nao indexado",
                    kind.label(),
                    item.rel
                ));
                return Ok(());
            }
            content.clone()
        }
        SecretPolicy::Mask => {
            let (masked, count) = security::mask(&content);
            if count > 0 {
                report.masked += 1;
            }
            masked
        }
    };

    let generated = provider.semantic_for(&item.rel, &prompt_content)?;
    report.bytes_sent += prompt_content.len() as u64;
    report.calls += 1;
    report.prompt_tokens += generated.prompt_tokens;
    report.completion_tokens += generated.completion_tokens;
    report.latency_ms += generated.latency_ms;

    // O arquivo mudou enquanto o modelo analisava? Nao grava semantica velha.
    let hash_after = hash::sha256_file(&item.path)?;
    if hash_after != hash_before {
        report.deferred += 1;
        return Ok(());
    }

    let metadata = fs::metadata(&item.path).at(&item.path)?;

    // Arestas estaticas: custo zero de inferencia (bead c2-e6).
    let extracted = links::extract(
        workspace.root(),
        &item.rel,
        &content,
        config.index.max_links_per_file,
    );

    let meta = MetaFile::new(
        item.rel.clone(),
        hash_after,
        metadata.len(),
        mtime_secs(&metadata),
        generated.semantic,
        extracted,
    );
    meta.write_atomic(&item.meta_path)?;
    report.indexed += 1;
    Ok(())
}

fn extract_links(
    workspace: &Workspace,
    config: &Config,
    item: &PlanItem,
) -> Result<Vec<crate::schema::Link>> {
    let content = fs::read_to_string(&item.path).at(&item.path)?;
    Ok(links::extract(
        workspace.root(),
        &item.rel,
        &content,
        config.index.max_links_per_file,
    ))
}

fn record_error(report: &mut IndexReport, err: C2Error) {
    report.errors += 1;
    report.last_error = Some(err.to_string());
}

fn mtime_secs(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Atualiza, em todos os metadados, links que apontavam para o caminho antigo.
fn relink_references(workspace: &Workspace, old_rel: &str, new_rel: &str) -> Result<usize> {
    let mut alterados = 0;
    for caminho in crate::cleaner::metadata_files(workspace.root())? {
        let Ok(mut meta) = MetaFile::read(&caminho) else {
            continue;
        };
        let mut mudou = false;
        for link in &mut meta.links {
            if link.to == old_rel {
                link.to = new_rel.to_string();
                mudou = true;
            }
        }
        if mudou {
            meta.write_atomic(&caminho)?;
            alterados += 1;
        }
    }
    Ok(alterados)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Generated;
    use crate::schema::Semantic;
    use crate::workspace::Workspace;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockProvider {
        calls: AtomicUsize,
        fail: bool,
        on_call: Option<Box<dyn Fn() + Send + Sync>>,
    }

    /// Guarda o conteudo recebido, para provar que o segredo nao foi enviado.
    #[derive(Default)]
    struct CapturingProvider {
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl SemanticProvider for CapturingProvider {
        fn semantic_for(&self, _rel: &str, content: &str) -> Result<Generated> {
            self.seen.lock().unwrap().push(content.to_string());
            Ok(Generated {
                semantic: Semantic {
                    what: "w".into(),
                    why: "y".into(),
                    goal: "g".into(),
                    summary: "s".into(),
                    responsibility: "r".into(),
                },
                prompt_tokens: 1,
                completion_tokens: 1,
                latency_ms: 1,
                retries: 0,
            })
        }
    }

    impl MockProvider {
        fn ok() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail: false,
                on_call: None,
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl SemanticProvider for MockProvider {
        fn semantic_for(&self, _rel: &str, _content: &str) -> Result<Generated> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(callback) = &self.on_call {
                callback();
            }
            if self.fail {
                return Err(C2Error::Llm("modelo indisponivel".to_string()));
            }
            Ok(Generated {
                semantic: Semantic {
                    what: "o que".into(),
                    why: "por que".into(),
                    goal: "objetivo".into(),
                    summary: "resumo".into(),
                    responsibility: "dono".into(),
                },
                prompt_tokens: 10,
                completion_tokens: 5,
                latency_ms: 100,
                retries: 0,
            })
        }
    }

    fn setup(tag: &str) -> (PathBuf, Workspace) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("c2-idx-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(base.join("src/b.rs"), "fn b() {}\n").unwrap();
        let workspace = Workspace::at(&base);
        workspace.init(false).unwrap();
        (base, workspace)
    }

    fn options() -> IndexOptions {
        IndexOptions::default()
    }

    fn opcoes_full() -> IndexOptions {
        IndexOptions {
            full: true,
            ..IndexOptions::default()
        }
    }

    #[test]
    fn cache_de_hash_evita_recalculo_na_segunda_passada() {
        let (base, workspace) = setup("cache-hit");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();

        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert!(workspace.hash_cache_path().is_file(), "cache deve existir");

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.unchanged, 2);
        assert_eq!(report.hash_cache_hits, 2, "os dois hashes vem do cache");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn cache_e_invalidado_quando_o_arquivo_muda() {
        let (base, workspace) = setup("cache-miss");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        // tamanho e mtime diferentes: cache nao pode ser usado para este arquivo
        fs::write(base.join("src/a.rs"), "fn a() { let x = 1; }\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.indexed, 1, "so o arquivo alterado e reindexado");
        assert_eq!(report.hash_cache_hits, 1, "o outro vem do cache");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn full_ignora_o_cache_de_hash() {
        let (base, workspace) = setup("cache-full");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        let report = run(&workspace, &config, &provider, &opcoes_full(), |_| {}).unwrap();
        assert_eq!(
            report.hash_cache_hits, 0,
            "--full recalcula todos os hashes"
        );
        assert_eq!(
            provider.calls(),
            4,
            "full reindexa os 2 de novo (2 antes + 2 agora)"
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn renomear_arquivo_reaproveita_metadado_sem_llm() {
        let (base, workspace) = setup("rename");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(provider.calls(), 2);

        fs::rename(base.join("src/a.rs"), base.join("src/c.rs")).unwrap();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        assert_eq!(report.renamed, 1);
        assert_eq!(report.removed, 0);
        assert_eq!(report.indexed, 0);
        assert_eq!(provider.calls(), 2, "rename nao pode chamar o modelo");

        assert!(base.join("src/c.rs.c2.json").is_file());
        assert!(!base.join("src/a.rs.c2.json").exists());
        let meta = MetaFile::read(&base.join("src/c.rs.c2.json")).unwrap();
        assert_eq!(meta.file, "src/c.rs");
        assert_eq!(meta.semantic.what, "o que");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn renomear_atualiza_links_de_outros_metadados() {
        let (base, workspace) = setup("rename-links");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        // b.rs passa a apontar para a.rs
        let caminho_b = base.join("src/b.rs.c2.json");
        let mut meta_b = MetaFile::read(&caminho_b).unwrap();
        meta_b.links = vec![crate::schema::Link {
            to: "src/a.rs".to_string(),
            kind: "uses".to_string(),
        }];
        meta_b.write_atomic(&caminho_b).unwrap();

        fs::rename(base.join("src/a.rs"), base.join("src/c.rs")).unwrap();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        assert_eq!(report.renamed, 1);
        assert!(report.relinked >= 1, "link antigo deve ser reescrito");
        let meta_b = MetaFile::read(&caminho_b).unwrap();
        assert_eq!(meta_b.links[0].to, "src/c.rs");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn indexa_arquivos_novos() {
        let (base, workspace) = setup("novo");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.indexed, 2);
        assert_eq!(provider.calls(), 2);

        let meta = MetaFile::read(&base.join("src/a.rs.c2.json")).unwrap();
        assert_eq!(meta.semantic.what, "o que");
        assert_eq!(meta.file, "src/a.rs");
        assert!(meta.links.is_empty());
        assert_eq!(meta.schema_version, 1);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn segunda_passada_nao_chama_o_modelo() {
        let (base, workspace) = setup("idem");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();

        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(provider.calls(), 2);

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.unchanged, 2);
        assert_eq!(report.indexed, 0);
        assert_eq!(provider.calls(), 2, "nao deve chamar o modelo de novo");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn alteracao_reindexa_somente_o_arquivo_mudado() {
        let (base, workspace) = setup("alt");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        fs::write(base.join("src/a.rs"), "fn a() { mudou(); }\n").unwrap();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.indexed, 1);
        assert_eq!(report.unchanged, 1);
        assert_eq!(provider.calls(), 3);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn remocao_limpa_metadado() {
        let (base, workspace) = setup("rm");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        fs::remove_file(base.join("src/b.rs")).unwrap();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!base.join("src/b.rs.c2.json").exists());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn full_forca_reindexacao() {
        let (base, workspace) = setup("full");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        let options = IndexOptions {
            full: true,
            ..Default::default()
        };
        let report = run(&workspace, &config, &provider, &options, |_| {}).unwrap();
        assert_eq!(report.indexed, 2);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn erro_do_modelo_nao_grava_metadado() {
        let (base, workspace) = setup("erro");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider {
            calls: AtomicUsize::new(0),
            fail: true,
            on_call: None,
        };

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.errors, 2);
        assert!(!base.join("src/a.rs.c2.json").exists());
        assert!(report.last_error.is_some());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn arquivo_alterado_durante_analise_fica_para_depois() {
        let (base, workspace) = setup("durante");
        let config = workspace.load_config().unwrap();
        let target = base.join("src/a.rs");
        let provider = MockProvider {
            calls: AtomicUsize::new(0),
            fail: false,
            on_call: Some(Box::new(move || {
                // Simula edicao do usuario durante a inferencia.
                let _ = fs::write(&target, "fn a() { editado durante a analise(); }\n");
            })),
        };

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert!(report.deferred >= 1, "deve adiar arquivo alterado no meio");
        assert_eq!(report.errors, 0);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn dry_run_nao_escreve_nada() {
        let (base, workspace) = setup("dry");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        let options = IndexOptions {
            dry_run: true,
            ..Default::default()
        };

        let report = run(&workspace, &config, &provider, &options, |_| {}).unwrap();
        assert_eq!(report.indexed, 0);
        assert!(!base.join("src/a.rs.c2.json").exists());
        assert_eq!(provider.calls(), 0);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn grava_links_estaticos_no_metadado() {
        let (base, workspace) = setup("links");
        let config = workspace.load_config().unwrap();
        fs::write(base.join("src/a.rs"), "mod b;\nfn a() {}\n").unwrap();
        let provider = MockProvider::ok();

        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        let meta = MetaFile::read(&base.join("src/a.rs.c2.json")).unwrap();
        assert!(
            meta.links
                .iter()
                .any(|l| l.to == "src/b.rs" && l.kind == "mod"),
            "links: {:?}",
            meta.links
        );

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn completa_links_de_metadado_existente_sem_chamar_o_modelo() {
        let (base, workspace) = setup("relink");
        let config = workspace.load_config().unwrap();
        fs::write(base.join("src/a.rs"), "mod b;\nfn a() {}\n").unwrap();
        let provider = MockProvider::ok();
        run(&workspace, &config, &provider, &options(), |_| {}).unwrap();

        // Simula metadado de versao anterior do extrator: sem arestas.
        let meta_path = base.join("src/a.rs.c2.json");
        let mut meta = MetaFile::read(&meta_path).unwrap();
        meta.links.clear();
        meta.write_atomic(&meta_path).unwrap();

        let calls_before = provider.calls();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.relinked, 1);
        assert_eq!(provider.calls(), calls_before, "relink nao chama o modelo");
        let meta = MetaFile::read(&meta_path).unwrap();
        assert!(!meta.links.is_empty());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn arquivo_com_segredo_nao_e_indexado() {
        let (base, workspace) = setup("secret");
        let config = workspace.load_config().unwrap();
        fs::write(
            base.join("src/a.rs"),
            "-----BEGIN RSA PRIVATE KEY-----\nabc\n", // c2:allow-secret
        )
        .unwrap();
        let provider = MockProvider::ok();

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.blocked, 1);
        assert_eq!(report.indexed, 1, "b.rs continua sendo indexado");
        assert!(!base.join("src/a.rs.c2.json").exists());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn politica_mask_envia_conteudo_mascarado() {
        let (base, workspace) = setup("mask");
        let mut config = workspace.load_config().unwrap();
        config.index.secret_policy = crate::security::SecretPolicy::Mask;
        fs::write(
            base.join("src/a.rs"),
            "password = \"super-secreta-123\"\nfn a() {}\n", // c2:allow-secret
        )
        .unwrap();
        let provider = CapturingProvider::default();

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.masked, 1);

        let seen = provider.seen.lock().unwrap();
        let all: String = seen.join("\n");
        assert!(all.contains("[REDACTED]"), "conteudo enviado: {all}");
        assert!(!all.contains("super-secreta-123"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn allowlist_libera_arquivo_com_fixture_de_segredo() {
        let (base, workspace) = setup("allow");
        let mut config = workspace.load_config().unwrap();
        config.index.secret_allow = vec!["src/a.rs".to_string()];
        fs::write(
            base.join("src/a.rs"),
            "password = \"fixture-de-teste-123\"\n",
        )
        .unwrap();
        let provider = MockProvider::ok();

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert_eq!(report.blocked, 0);
        assert!(base.join("src/a.rs.c2.json").exists());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn teto_de_bytes_por_ciclo_adia_o_resto() {
        let (base, workspace) = setup("budget");
        let mut config = workspace.load_config().unwrap();
        config.index.max_total_bytes_per_run = 12;
        let provider = MockProvider::ok();

        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        assert!(report.deferred >= 1, "deve adiar o que passa do teto");
        assert!(report.bytes_sent <= 12);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn limite_processa_apenas_n_arquivos() {
        let (base, workspace) = setup("limit");
        let config = workspace.load_config().unwrap();
        let provider = MockProvider::ok();
        let options = IndexOptions {
            limit: Some(1),
            ..Default::default()
        };

        let report = run(&workspace, &config, &provider, &options, |_| {}).unwrap();
        assert_eq!(report.indexed, 1);
        assert_eq!(provider.calls(), 1);

        fs::remove_dir_all(&base).ok();
    }
    /// Mede o efeito do cache de hash: primeira passada x segunda (tudo igual).
    /// Rode com: cargo test -p c2-core --lib bench_cache -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_cache_de_hash() {
        let (base, workspace) = setup("bench-cache");
        let mut config = workspace.load_config().unwrap();
        config.index.max_total_bytes_per_run = 0;
        for indice in 0..300 {
            fs::write(
                base.join(format!("src/f{indice:03}.rs")),
                format!("pub fn f{indice}() -> u32 {{ {indice} }}\n"),
            )
            .unwrap();
        }
        let provider = MockProvider::ok();

        let inicio = std::time::Instant::now();
        let primeiro = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        let primeira = inicio.elapsed();

        let inicio = std::time::Instant::now();
        let report = run(&workspace, &config, &provider, &options(), |_| {}).unwrap();
        let segunda = inicio.elapsed();

        println!(
            "300 arquivos: 1a passada {primeira:?} ({} indexados, {} chamadas), 2a passada {segunda:?} ({} hashes do cache, {} inalterados)",
            primeiro.indexed, primeiro.calls, report.hash_cache_hits, report.unchanged
        );
        fs::remove_dir_all(&base).ok();
    }
}
