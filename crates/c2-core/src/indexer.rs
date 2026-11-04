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
    Removed,
}

impl Decision {
    pub fn label(self) -> &'static str {
        match self {
            Decision::New => "novo",
            Decision::Changed => "alterado",
            Decision::Unchanged => "inalterado",
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
            + self.blocked
            + self.masked
    }
}

/// Monta o plano: descobre arquivos, compara com os metadados e lista orfaos.
pub fn plan(
    workspace: &Workspace,
    config: &Config,
    full: bool,
    only_path: Option<&str>,
) -> Result<Vec<PlanItem>> {
    let root = workspace.root();
    let discovery = discover::discover(root, config)?;
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

        let (decision, current_hash) = match &existing {
            None => (Decision::New, Some(hash::sha256_file(&entry.path)?)),
            Some(meta) => {
                let current = hash::sha256_file(&entry.path)?;
                if !full && meta.hash == current {
                    (Decision::Unchanged, None)
                } else {
                    (Decision::Changed, Some(current))
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
            hash: current_hash,
            existing,
        });
    }

    // Orfaos: metadado cujo arquivo-fonte nao existe mais.
    let mut seen: BTreeSet<PathBuf> = items.iter().map(|i| i.meta_path.clone()).collect();
    for meta_path in crate::cleaner::metadata_files(root)? {
        if !seen.insert(meta_path.clone()) {
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
        items.push(PlanItem {
            rel,
            path: source,
            meta_path,
            decision: Decision::Removed,
            size: 0,
            mtime: 0,
            hash: None,
            existing: None,
        });
    }

    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(items)
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
    let mut report = IndexReport::default();
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
    let prompt_content = match config.index.secret_policy {
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
            "-----BEGIN RSA PRIVATE KEY-----\nabc\n",
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
            "password = \"super-secreta-123\"\nfn a() {}\n",
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
}
