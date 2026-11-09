//! Ponte entre a janela (frontend) e o nucleo do C2.
//!
//! Toda a logica vive em `c2-core`; aqui so adaptamos chamadas e eventos.
//! Ver `docs/adr/0001-arquitetura-em-camadas.md`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use c2_core::indexer::{self, IndexOptions, IndexReport};
use c2_core::llm::LlmClient;
use c2_core::search::{self, SearchOptions};
use c2_core::status::{self, StatusReport};
use c2_core::{cleaner, runtime, EngineState, MetaFile, Workspace};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;

mod perf;

/// Projeto passado por `c2 open --root <dir>`.
struct InitialRoot(Option<String>);

/// Estado do laco de indexacao controlado pela janela.
#[derive(Default)]
struct Engine {
    running: Arc<AtomicBool>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Cancelamento do reindex manual (botao Reindexar tudo).
#[derive(Default)]
struct Reindex {
    cancel: Arc<AtomicBool>,
}

/// Ultimo workspace aberto, para limpar o runtime ao sair pelo tray.
#[derive(Default)]
struct LastWorkspace(Mutex<Option<String>>);

/// Recusa iniciar o engine sem um `.gguf` valido configurado.
fn require_model(config: &c2_core::Config) -> Result<(), String> {
    match config.model.path.as_deref() {
        Some(path) if !path.trim().is_empty() => {
            if Path::new(path).is_file() {
                Ok(())
            } else {
                Err(format!("modelo GGUF nao encontrado: {path}"))
            }
        }
        _ => Err("nenhum modelo .gguf selecionado (aba Modelo)".to_string()),
    }
}

fn open_workspace(root: &str) -> Result<Workspace, String> {
    let path = c2_core::workspace::canonical_path(Path::new(root)).map_err(|err| err.to_string())?;
    let workspace = Workspace::at(path);
    // O usuario pode escolher uma subpasta: sobe ate a raiz inicializada.
    if let Ok(Some(found)) = Workspace::find(workspace.root()) {
        return Ok(found);
    }
    Ok(workspace)
}

/// Preferencias de modelo que valem para o app inteiro (nao por projeto).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ModeloGlobal {
    path: Option<String>,
    name: Option<String>,
    binary: Option<String>,
    ctx_size: Option<u32>,
    gpu_layers: Option<u32>,
    threads: Option<u32>,
    disable_thinking: Option<bool>,
}

fn caminho_modelo_global(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("global-model.json"))
        .map_err(|erro| erro.to_string())
}

fn ler_modelo_global(app: &tauri::AppHandle) -> ModeloGlobal {
    let Ok(caminho) = caminho_modelo_global(app) else {
        return ModeloGlobal::default();
    };
    std::fs::read_to_string(caminho)
        .ok()
        .and_then(|texto| serde_json::from_str(&texto).ok())
        .unwrap_or_default()
}

fn salvar_modelo_global(app: &tauri::AppHandle, modelo: &ModeloGlobal) -> Result<(), String> {
    let caminho = caminho_modelo_global(app)?;
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai).map_err(|erro| erro.to_string())?;
    }
    let texto = serde_json::to_string_pretty(modelo).map_err(|erro| erro.to_string())?;
    std::fs::write(&caminho, texto).map_err(|erro| erro.to_string())
}

fn capturar_modelo_global(config: &c2_core::config::Config) -> ModeloGlobal {
    ModeloGlobal {
        path: config.model.path.clone(),
        name: Some(config.model.name.clone()),
        binary: config.model.binary.clone(),
        ctx_size: Some(config.model.ctx_size),
        gpu_layers: Some(config.model.gpu_layers),
        threads: Some(config.model.threads),
        disable_thinking: Some(config.model.disable_thinking),
    }
}

/// Se o projeto ainda nao tem modelo, aplica o modelo global (set once).
fn aplicar_modelo_global(app: &tauri::AppHandle, workspace: &Workspace) -> Result<bool, String> {
    if !workspace.is_initialized() {
        return Ok(false);
    }
    let mut config = workspace.load_config().map_err(|erro| erro.to_string())?;
    let tem_modelo = config
        .model
        .path
        .as_deref()
        .map(|valor| !valor.trim().is_empty())
        .unwrap_or(false);
    if tem_modelo {
        return Ok(false);
    }
    let global = ler_modelo_global(app);
    let Some(caminho) = global.path.filter(|valor| Path::new(valor).is_file()) else {
        return Ok(false);
    };
    config.model.path = Some(caminho);
    if let Some(nome) = global.name.filter(|valor| !valor.trim().is_empty()) {
        config.model.name = nome;
    }
    if let Some(binario) = global.binary {
        config.model.binary = Some(binario);
    }
    if let Some(valor) = global.ctx_size {
        config.model.ctx_size = valor;
    }
    if let Some(valor) = global.gpu_layers {
        config.model.gpu_layers = valor;
    }
    if let Some(valor) = global.threads {
        config.model.threads = valor;
    }
    if let Some(valor) = global.disable_thinking {
        config.model.disable_thinking = valor;
    }
    workspace
        .save_config(&config)
        .map_err(|erro| erro.to_string())?;
    Ok(true)
}

/// Preferencias globais do modelo de embeddings (bge-m3).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct EmbeddingGlobal {
    path: Option<String>,
    model_id: Option<String>,
    dim: Option<usize>,
    device: Option<String>,
}

fn caminho_embedding_global(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("global-embedding.json"))
        .map_err(|erro| erro.to_string())
}

fn ler_embedding_global(app: &tauri::AppHandle) -> EmbeddingGlobal {
    let Ok(caminho) = caminho_embedding_global(app) else {
        return EmbeddingGlobal::default();
    };
    std::fs::read_to_string(caminho)
        .ok()
        .and_then(|texto| serde_json::from_str(&texto).ok())
        .unwrap_or_default()
}

fn salvar_embedding_global(app: &tauri::AppHandle, modelo: &EmbeddingGlobal) -> Result<(), String> {
    let caminho = caminho_embedding_global(app)?;
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai).map_err(|erro| erro.to_string())?;
    }
    let texto = serde_json::to_string_pretty(modelo).map_err(|erro| erro.to_string())?;
    std::fs::write(&caminho, texto).map_err(|erro| erro.to_string())
}

fn aplicar_embedding_global(app: &tauri::AppHandle, workspace: &Workspace) -> Result<(), String> {
    if !workspace.is_initialized() {
        return Ok(());
    }
    let mut config = workspace.load_config().map_err(|erro| erro.to_string())?;
    if config
        .index
        .embeddings
        .model_path
        .as_deref()
        .map(|valor| !valor.trim().is_empty())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let global = ler_embedding_global(app);
    let Some(path) = global.path.filter(|valor| Path::new(valor).is_file()) else {
        return Ok(());
    };
    config.index.embeddings.model_path = Some(path);
    if let Some(model_id) = global.model_id {
        config.index.embeddings.model_id = model_id;
    }
    if let Some(dim) = global.dim {
        config.index.embeddings.dim = dim;
    }
    if let Some(device) = global.device {
        config.index.embeddings.device = device;
    }
    workspace
        .save_config(&config)
        .map_err(|erro| erro.to_string())
}

#[tauri::command]
fn embedding_status(
    app: tauri::AppHandle,
    root: String,
) -> Result<c2_core::embedding_runtime::EmbeddingStatus, String> {
    let workspace = open_workspace(&root)?;
    aplicar_embedding_global(&app, &workspace)?;
    let config = workspace.load_config().map_err(|erro| erro.to_string())?;
    Ok(c2_core::embedding_runtime::status(&workspace, &config))
}

#[tauri::command]
fn embedding_save(
    app: tauri::AppHandle,
    root: String,
    model_path: Option<String>,
    enabled: Option<bool>,
    device: Option<String>,
) -> Result<c2_core::embedding_runtime::EmbeddingStatus, String> {
    let workspace = open_workspace(&root)?;
    let mut config = workspace.load_config().map_err(|erro| erro.to_string())?;
    if let Some(path) = model_path {
        if !path.trim().is_empty() && !Path::new(&path).is_file() {
            return Err(format!("arquivo GGUF nao encontrado: {path}"));
        }
        config.index.embeddings.model_path = Some(path);
    }
    if let Some(enabled) = enabled {
        config.index.embeddings.enabled = enabled;
    }
    if let Some(device) = device {
        if device != "cpu" && device != "gpu" {
            return Err("device deve ser cpu ou gpu".to_string());
        }
        config.index.embeddings.device = device;
    }
    workspace.save_config(&config).map_err(|erro| erro.to_string())?;
    let global = EmbeddingGlobal {
        path: config.index.embeddings.model_path.clone(),
        model_id: Some(config.index.embeddings.model_id.clone()),
        dim: Some(config.index.embeddings.dim),
        device: Some(config.index.embeddings.device.clone()),
    };
    salvar_embedding_global(&app, &global)?;
    Ok(c2_core::embedding_runtime::status(&workspace, &config))
}

#[tauri::command]
fn embedding_start(root: String) -> Result<c2_core::embedding_runtime::EmbeddingStatus, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|erro| erro.to_string())?;
    c2_core::embedding_runtime::ensure(&workspace, &config).map_err(|err| err.to_string())
}

#[tauri::command]
fn embedding_stop(root: String) -> Result<bool, String> {
    let workspace = open_workspace(&root)?;
    c2_core::embedding_runtime::stop(&workspace).map_err(|err| err.to_string())
}

#[tauri::command]
async fn embedding_index(
    root: String,
    full: Option<bool>,
) -> Result<c2_core::embedding_index::EmbeddingIndexReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = open_workspace(&root)?;
        let config = workspace.load_config().map_err(|erro| erro.to_string())?;
        c2_core::embedding_index::run(&workspace, &config, full.unwrap_or(false), |_, _, _| {})
            .map_err(|erro| erro.to_string())
    })
    .await
    .map_err(|erro| erro.to_string())?
}

#[tauri::command]
fn embedding_pick_model(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .add_filter("Modelo GGUF", &["gguf"])
        .blocking_pick_file()
        .map(|path| path.to_string())
}

/// Projeto com que a janela foi aberta (se veio de `c2 open`).
#[tauri::command]
fn initial_root(initial: State<'_, InitialRoot>) -> Option<String> {
    initial.0.clone()
}

#[tauri::command]
async fn workspace_status(
    app: tauri::AppHandle,
    root: String,
    ultimo: State<'_, LastWorkspace>,
) -> Result<Option<StatusReport>, String> {
    if let Ok(mut guard) = ultimo.0.lock() {
        *guard = Some(root.clone());
    }
    let inicio = std::time::Instant::now();
    let app_dentro = app.clone();
    let root_dentro = root.clone();
    // A varredura do projeto e a leitura de metadados sao bloqueantes: fora da
    // thread da UI para a janela continuar respondendo durante o cold start.
    let (resultado, timings) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(Option<StatusReport>, Option<status::StatusTimings>), String> {
            let workspace = open_workspace(&root_dentro)?;
            if !workspace.is_initialized() {
                return Ok((None, None));
            }
            // Modelo e preferencia do app: se o projeto nao tem, herda o global.
            aplicar_modelo_global(&app_dentro, &workspace)?;
        aplicar_embedding_global(&app_dentro, &workspace)?;
            let (report, timings) =
                status::build_timed(&workspace).map_err(|err| err.to_string())?;
            Ok((Some(report), Some(timings)))
        },
    )
    .await
    .map_err(|err| err.to_string())??;
    perf::registrar(
        &app,
        "workspace_status",
        serde_json::json!({
            "ms": inicio.elapsed().as_millis() as u64,
            "initialized": resultado.is_some(),
            "timings": timings,
        }),
    );
    Ok(resultado)
}

#[derive(Serialize)]
struct InitResult {
    created: bool,
    files: usize,
}

#[tauri::command]
fn workspace_init(app: tauri::AppHandle, root: String) -> Result<InitResult, String> {
    let path = c2_core::workspace::canonical_path(Path::new(&root)).map_err(|err| err.to_string())?;
    let workspace = Workspace::at(path);
    let outcome = workspace.init(false).map_err(|err| err.to_string())?;
    let _ = aplicar_modelo_global(&app, &workspace);
    Ok(InitResult {
        created: outcome.created,
        files: outcome.files,
    })
}

/// Uma passada completa: sobe runtime, indexa, atualiza estado e para o runtime.
#[tauri::command]
async fn workspace_reindex(
    app: tauri::AppHandle,
    root: String,
    full: Option<bool>,
    limit: Option<usize>,
    reindex: State<'_, Reindex>,
) -> Result<IndexReport, String> {
    let cancel = Arc::clone(&reindex.cancel);
    cancel.store(false, Ordering::SeqCst);
    let app_progresso = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = open_workspace(&root)?;
        let mut config = workspace.load_config().map_err(|err| err.to_string())?;
        require_model(&config)?;
        let logger = c2_core::log::Logger::for_workspace(&workspace, &config);
        logger.info("desktop", "reindex iniciado");
        if c2_core::config::usa_runtime_local(&config) {
            runtime::ensure_auto(&workspace, &mut config).map_err(|err| err.to_string())?;
        }
        let client = LlmClient::from_config(&config);

        let options = IndexOptions {
            full: full.unwrap_or(false),
            limit,
            only_path: None,
            dry_run: false,
            stop: Some(Arc::clone(&cancel)),
            ..IndexOptions::default()
        };
        let report = indexer::run(&workspace, &config, &client, &options, |progress| {
            let _ = app_progresso.emit(
                "reindex://progress",
                serde_json::json!({
                    "index": progress.index,
                    "total": progress.total,
                    "rel": progress.rel,
                    "decision": progress.decision.label(),
                }),
            );
        })
        .map_err(|err| err.to_string())?;

        let mut state = EngineState::load(&workspace.state_path()).map_err(|err| err.to_string())?;
        state.merge_report(&report);
        state
            .save(&workspace.state_path())
            .map_err(|err| err.to_string())?;

        logger.info(
            "desktop",
            format!(
                "reindex concluido: {} indexados, {} erros, {} chamadas",
                report.indexed, report.errors, report.calls
            ),
        );
        if let Some(erro) = &report.last_error {
            logger.warn("desktop", format!("ultimo erro: {erro}"));
        }

        let _ = runtime::stop(&workspace);
        Ok(report)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
fn reindex_cancel(reindex: State<'_, Reindex>) {
    reindex.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn engine_start(
    root: String,
    app: tauri::AppHandle,
    engine: State<'_, Engine>,
) -> Result<(), String> {
    if engine.running.load(Ordering::SeqCst) {
        return Ok(());
    }

    let workspace = open_workspace(&root)?;
    let mut config = workspace.load_config().map_err(|err| err.to_string())?;
    require_model(&config)?;
    let logger = c2_core::log::Logger::for_workspace(&workspace, &config);
    let running = Arc::clone(&engine.running);
    running.store(true, Ordering::SeqCst);

    let handle = std::thread::spawn(move || {
        if c2_core::config::usa_runtime_local(&config) {
            if let Err(err) = runtime::ensure_auto(&workspace, &mut config) {
                logger.error("engine", format!("falha ao subir o runtime: {err}"));
                let _ = app.emit("engine://error", err.to_string());
                running.store(false, Ordering::SeqCst);
                return;
            }
        }
        let mut client = LlmClient::from_config(&config);
        logger.info("engine", "engine iniciado");

        let interval = Duration::from_secs(config.engine.poll_interval_secs.max(1));
        while running.load(Ordering::SeqCst) {
            let report = indexer::run(&workspace, &config, &client, &IndexOptions::default(), |progress| {
                let _ = app.emit(
                    "engine://progress",
                    serde_json::json!({
                        "rel": progress.rel,
                        "decision": progress.decision.label(),
                        "index": progress.index,
                        "total": progress.total,
                    }),
                );
            });

            match report {
                Ok(report) => {
                    let mut state = EngineState::load(&workspace.state_path()).unwrap_or_default();
                    state.merge_report(&report);
                    let _ = state.save(&workspace.state_path());
                    logger.info(
                        "engine",
                        format!(
                            "ciclo: {} indexados, {} inalterados, {} erros, {} chamadas",
                            report.indexed, report.unchanged, report.errors, report.calls
                        ),
                    );
                    if let Some(erro) = &report.last_error {
                        logger.warn("engine", format!("erro no ciclo: {erro}"));
                    }
                    let _ = app.emit("engine://cycle", report);
                }
                Err(err) => {
                    logger.error("engine", format!("falha no ciclo: {err}"));
                    let _ = app.emit("engine://error", err.to_string());
                }
            }

            // Watchdog: servidor caiu? sobe de novo (com backoff interno).
            if running.load(Ordering::SeqCst)
                && c2_core::config::usa_runtime_local(&config)
                && !runtime::health(&config)
            {
                let porta_antes = config.model.port;
                let _ = runtime::ensure_auto(&workspace, &mut config);
                if config.model.port != porta_antes {
                    client = LlmClient::from_config(&config);
                }
            }
            std::thread::sleep(interval);
        }

        logger.info("engine", "engine encerrado (modelo mantido na GPU)");
        // Nao para o runtime: o modelo continua na VRAM para o banner
        // "Model on GPU loaded" e para religar o engine rapidamente.
        let _ = app.emit("engine://stopped", ());
    });

    *engine.handle.lock().map_err(|err| err.to_string())? = Some(handle);
    Ok(())
}

#[tauri::command]
fn engine_stop(_root: String, engine: State<'_, Engine>) -> Result<(), String> {
    engine.running.store(false, Ordering::SeqCst);
    // O runtime/modelo permanece na GPU; o banner fica em "Model on GPU loaded"
    // (estado idle) e um novo Start reaproveita o runtime saudavel.
    Ok(())
}

#[tauri::command]
fn model_unload(root: String, engine: State<'_, Engine>) -> Result<(), String> {
    engine.running.store(false, Ordering::SeqCst);
    let workspace = open_workspace(&root)?;
    runtime::stop(&workspace).map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn engine_running(engine: State<'_, Engine>) -> bool {
    engine.running.load(Ordering::SeqCst)
}

#[tauri::command]
fn search_grep(
    root: String,
    query: String,
    limit: Option<usize>,
    hybrid: Option<bool>,
) -> Result<Vec<search::Hit>, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    let options = SearchOptions {
        limit: limit.unwrap_or(10),
        include_snippets: true,
        hybrid: hybrid.unwrap_or(false),
    };
    search::search(&workspace, &config, &query, &options).map_err(|err| err.to_string())
}

#[derive(Serialize)]
struct ReadResult {
    file: String,
    line_start: usize,
    line_end: usize,
    lines: Vec<String>,
}

#[tauri::command]
fn file_read(
    root: String,
    file: String,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<ReadResult, String> {
    let workspace = open_workspace(&root)?;
    let rel = file.replace('\\', "/");
    let path = workspace
        .root()
        .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let safe = workspace.ensure_inside(&path).map_err(|err| err.to_string())?;
    let content = std::fs::read_to_string(&safe).map_err(|err| err.to_string())?;
    let all: Vec<&str> = content.lines().collect();
    if all.is_empty() {
        return Ok(ReadResult {
            file: rel,
            line_start: 1,
            line_end: 1,
            lines: vec![],
        });
    }
    let line_start = start.unwrap_or(1).max(1).min(all.len());
    let line_end = end.unwrap_or((line_start + 200).min(all.len())).min(all.len());
    Ok(ReadResult {
        file: rel,
        line_start,
        line_end,
        lines: all[line_start - 1..line_end]
            .iter()
            .map(|line| line.to_string())
            .collect(),
    })
}

#[derive(Serialize)]
struct RelatedLink {
    to: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
struct RelatedResult {
    file: String,
    uses: Vec<RelatedLink>,
    used_by: Vec<RelatedLink>,
}

#[tauri::command]
fn file_related(root: String, file: String) -> Result<RelatedResult, String> {
    let workspace = open_workspace(&root)?;
    let rel = file.replace('\\', "/");

    let mut all = Vec::new();
    for path in cleaner::metadata_files(workspace.root()).map_err(|err| err.to_string())? {
        if let Ok(meta) = MetaFile::read(&path) {
            all.push(meta);
        }
    }

    let target = all
        .iter()
        .find(|meta| meta.file == rel)
        .ok_or_else(|| format!("nenhum metadado para {rel}; reindexe o projeto"))?;

    let uses = target
        .links
        .iter()
        .map(|link| RelatedLink {
            to: link.to.clone(),
            kind: link.kind.clone(),
        })
        .collect();

    let mut used_by = Vec::new();
    for meta in &all {
        if meta.file == rel {
            continue;
        }
        for link in &meta.links {
            if link.to == rel {
                used_by.push(RelatedLink {
                    to: meta.file.clone(),
                    kind: link.kind.clone(),
                });
            }
        }
    }

    Ok(RelatedResult {
        file: rel,
        uses,
        used_by,
    })
}

#[derive(Serialize)]
struct FileEntry {
    file: String,
    what: String,
    summary: String,
    links: usize,
}

#[tauri::command]
fn list_files(root: String) -> Result<Vec<FileEntry>, String> {
    let workspace = open_workspace(&root)?;
    let mut entries = Vec::new();
    for path in cleaner::metadata_files(workspace.root()).map_err(|err| err.to_string())? {
        if let Ok(meta) = MetaFile::read(&path) {
            entries.push(FileEntry {
                file: meta.file.clone(),
                what: meta.semantic.what.clone(),
                summary: meta.semantic.summary.clone(),
                links: meta.links.len(),
            });
        }
    }
    entries.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(entries)
}

#[tauri::command]
fn read_meta(root: String, file: String) -> Result<MetaFile, String> {
    let workspace = open_workspace(&root)?;
    let rel = file.replace('\\', "/");
    let meta_path = c2_core::schema::meta_path_for(Path::new(
        &rel.replace('/', std::path::MAIN_SEPARATOR_STR),
    ));
    let full = workspace.root().join(meta_path);
    MetaFile::read(&full).map_err(|err| err.to_string())
}

#[tauri::command]
fn config_get(root: String) -> Result<serde_json::Value, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    serde_json::to_value(config).map_err(|err| err.to_string())
}

#[tauri::command]
fn config_set(app: tauri::AppHandle, root: String, key: String, value: String) -> Result<String, String> {
    let workspace = open_workspace(&root)?;
    let mut config = workspace.load_config().map_err(|err| err.to_string())?;
    config.set(&key, &value).map_err(|err| err.to_string())?;
    workspace.save_config(&config).map_err(|err| err.to_string())?;
    const CHAVES_MODELO: [&str; 7] = [
        "model.path",
        "model.name",
        "model.binary",
        "model.ctx_size",
        "model.gpu_layers",
        "model.threads",
        "model.disable_thinking",
    ];
    if CHAVES_MODELO.contains(&key.as_str()) {
        salvar_modelo_global(&app, &capturar_modelo_global(&config))?;
    }
    Ok(format!("{key} = {value}"))
}

#[tauri::command]
fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

#[derive(Serialize)]
struct ModelInspection {
    gguf: c2_core::gguf::GgufInfo,
    gpu: Option<c2_core::gpu::GpuInfo>,
    fit: c2_core::gpu::VramFit,
}

fn inspect_model(path: &str) -> Result<ModelInspection, String> {
    let gguf = c2_core::gguf::read_info(Path::new(path)).map_err(|err| err.to_string())?;
    let gpu = c2_core::gpu::detect();
    let fit = c2_core::gpu::vram_fit(gguf.size_bytes, gpu.as_ref());
    Ok(ModelInspection { gguf, gpu, fit })
}

/// Valida um `.gguf` pelo cabecalho (sem carregar o modelo) e devolve detalhes.
#[tauri::command]
fn model_inspect(path: String) -> Result<ModelInspection, String> {
    inspect_model(&path)
}

/// Grava `model.path` e o nome do modelo. Nunca copia o arquivo.
#[tauri::command]
fn model_set(app: tauri::AppHandle, root: String, path: String) -> Result<ModelInspection, String> {
    let workspace = open_workspace(&root)?;
    let inspection = inspect_model(&path)?;
    let mut config = workspace.load_config().map_err(|err| err.to_string())?;
    config.model.path = Some(path);
    if let Some(name) = &inspection.gguf.name {
        config.model.name = name.clone();
    }
    workspace
        .save_config(&config)
        .map_err(|err| err.to_string())?;
    salvar_modelo_global(&app, &capturar_modelo_global(&config))?;
    Ok(inspection)
}

#[derive(Serialize)]
struct RuntimePlan {
    gguf: c2_core::gguf::GgufInfo,
    gpu: Option<c2_core::gpu::GpuInfo>,
    ctx_size: u32,
    gpu_layers: u32,
    estimate: c2_core::gpu::UsageEstimate,
    /// Runtime ja esta no ar (modelo carregado).
    loaded: bool,
}

/// Plano de memoria para carregar o modelo na GPU (barra do modal).
#[tauri::command]
fn runtime_plan(
    root: String,
    ctx_size: Option<u32>,
    gpu_layers: Option<u32>,
) -> Result<RuntimePlan, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    let path = config
        .model
        .path
        .as_deref()
        .ok_or_else(|| "nenhum modelo .gguf selecionado".to_string())?;
    let gguf = c2_core::gguf::read_info(Path::new(path)).map_err(|err| err.to_string())?;

    let ctx = ctx_size.unwrap_or(config.model.ctx_size);
    let layers = gpu_layers.unwrap_or(config.model.gpu_layers);
    let gpu = c2_core::gpu::detect();
    let estimate = c2_core::gpu::estimate_usage(gguf.size_bytes, ctx, gpu.as_ref());

    Ok(RuntimePlan {
        gguf,
        gpu,
        ctx_size: ctx,
        gpu_layers: layers,
        estimate,
        loaded: runtime::health(&config),
    })
}

/// Grava contexto/camadas e sobe o llama-server (carrega o modelo na GPU).
#[tauri::command]
async fn model_load(
    app: tauri::AppHandle,
    root: String,
    ctx_size: u32,
    gpu_layers: u32,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = open_workspace(&root)?;
        let mut config = workspace.load_config().map_err(|err| err.to_string())?;
        require_model(&config)?;
        if !c2_core::config::usa_runtime_local(&config) {
            return Err("modelo local nao se aplica ao provedor de nuvem".to_string());
        }
        config.model.ctx_size = ctx_size;
        config.model.gpu_layers = gpu_layers;
        workspace
            .save_config(&config)
            .map_err(|err| err.to_string())?;
        salvar_modelo_global(&app, &capturar_modelo_global(&config))?;
        // O play chama model_load antes do engine_start: a troca de porta
        // automatica tambem precisa acontecer aqui.
        runtime::ensure_auto(&workspace, &mut config)
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[derive(serde::Serialize)]
struct McpToolInfo {
    name: String,
    description: String,
}

#[derive(serde::Serialize)]
struct McpStatus {
    available: bool,
    binary: Option<String>,
    protocol_version: Option<String>,
    server_name: Option<String>,
    tools: Vec<McpToolInfo>,
    error: Option<String>,
}

/// Localiza a CLI `c2` ao lado do app (pacote: <dest>/cli/c2.exe).
fn find_cli() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let nome = if cfg!(windows) { "c2.exe" } else { "c2" };
    let candidatos = [
        dir.join("cli").join(nome),
        dir.join("..").join("cli").join(nome),
        dir.join(nome),
    ];
    candidatos.into_iter().find(|caminho| caminho.is_file())
}

/// Sobe `c2 mcp`, faz o handshake e devolve o estado real do servidor.
#[tauri::command]
async fn mcp_smoke(root: String) -> McpStatus {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(cli) = find_cli() else {
            return McpStatus {
                available: false,
                binary: None,
                protocol_version: None,
                server_name: None,
                tools: Vec::new(),
                error: Some(
                    "CLI c2 nao encontrada ao lado do app (pacote: <dest>/cli/c2.exe)".to_string(),
                ),
            };
        };

        use std::io::{BufRead, BufReader, Write};
        let mut comando = std::process::Command::new(&cli);
        comando
            .arg("--root")
            .arg(&root)
            .arg("mcp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut filho = match c2_core::process::spawn(comando) {
            Ok(filho) => filho,
            Err(erro) => {
                return McpStatus {
                    available: false,
                    binary: Some(cli.display().to_string()),
                    protocol_version: None,
                    server_name: None,
                    tools: Vec::new(),
                    error: Some(format!("falha ao executar {}: {erro}", cli.display())),
                }
            }
        };

        let mut stdin = filho.stdin.take().expect("stdin do mcp");
        let stdout = filho.stdout.take().expect("stdout do mcp");
        let mut leitor = BufReader::new(stdout);

        let mut enviar = |linha: String| {
            let _ = writeln!(stdin, "{linha}");
            let _ = stdin.flush();
        };
        enviar(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"c2-desktop","version":"0.1.0"}}}"#.to_string());
        enviar(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string());

        let mut protocolo: Option<String> = None;
        let mut servidor: Option<String> = None;
        let mut ferramentas: Vec<McpToolInfo> = Vec::new();
        let mut erro: Option<String> = None;
        let mut lidas = 0;
        let mut linha = String::new();
        while lidas < 2 {
            linha.clear();
            match leitor.read_line(&mut linha) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            let Ok(valor) = serde_json::from_str::<serde_json::Value>(&linha) else {
                continue;
            };
            if valor.get("error").is_some() {
                erro = valor["error"]["message"].as_str().map(str::to_string);
            }
            if let Some(protocolo_valor) = valor.pointer("/result/protocolVersion").and_then(|v| v.as_str()) {
                protocolo = Some(protocolo_valor.to_string());
            }
            if let Some(nome) = valor.pointer("/result/serverInfo/name").and_then(|v| v.as_str()) {
                servidor = Some(nome.to_string());
            }
            if let Some(lista) = valor.pointer("/result/tools").and_then(|v| v.as_array()) {
                ferramentas = lista
                    .iter()
                    .map(|tool| McpToolInfo {
                        name: tool["name"].as_str().unwrap_or_default().to_string(),
                        description: tool["description"].as_str().unwrap_or_default().to_string(),
                    })
                    .collect();
            }
            lidas += 1;
        }

        let _ = filho.kill();
        let _ = filho.wait();

        McpStatus {
            available: protocolo.is_some() && !ferramentas.is_empty(),
            binary: Some(cli.display().to_string()),
            protocol_version: protocolo,
            server_name: servidor,
            tools: ferramentas,
            error: erro,
        }
    })
    .await
    .unwrap_or_else(|erro| McpStatus {
        available: false,
        binary: None,
        protocol_version: None,
        server_name: None,
        tools: Vec::new(),
        error: Some(format!("falha interna ao testar o MCP: {erro}")),
    })
}

#[derive(serde::Serialize)]
struct ProviderTest {
    ok: bool,
    provider: String,
    model: String,
    latency_ms: u64,
    sample: Option<String>,
    error: Option<String>,
}

/// Grava a API key no cofre do sistema (nunca no config.json).
#[tauri::command]
fn api_key_set(key: String) -> Result<(), String> {
    c2_core::secret::save(&key).map_err(|erro| erro.to_string())
}

#[tauri::command]
fn api_key_delete() -> Result<(), String> {
    c2_core::secret::delete().map_err(|erro| erro.to_string())
}

#[tauri::command]
fn api_key_status() -> bool {
    c2_core::secret::stored_in_keyring()
}

/// Testa o provedor configurado gerando a semantica de um arquivo minimo.
#[tauri::command]
async fn provider_test(root: String) -> ProviderTest {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = c2_core::workspace::Workspace::at(std::path::Path::new(&root));
        let config = match workspace.load_config() {
            Ok(config) => config,
            Err(erro) => {
                return ProviderTest {
                    ok: false,
                    provider: "?".to_string(),
                    model: "?".to_string(),
                    latency_ms: 0,
                    sample: None,
                    error: Some(erro.to_string()),
                }
            }
        };
        let provider = config.model.provider.label().to_string();
        let modelo = config.model.name.clone();
        let cliente = c2_core::llm::LlmClient::from_config(&config);
        let iniciado = std::time::Instant::now();
        match c2_core::llm::SemanticProvider::semantic_for(
            &cliente,
            "src/exemplo_teste.rs",
            "fn exemplo_teste() { let total = 2 + 2; }",
        ) {
            Ok(gerado) => ProviderTest {
                ok: true,
                provider,
                model: modelo,
                latency_ms: iniciado.elapsed().as_millis() as u64,
                sample: Some(gerado.semantic.summary),
                error: None,
            },
            Err(erro) => ProviderTest {
                ok: false,
                provider,
                model: modelo,
                latency_ms: iniciado.elapsed().as_millis() as u64,
                sample: None,
                error: Some(erro.to_string()),
            },
        }
    })
    .await
    .unwrap_or_else(|erro| ProviderTest {
        ok: false,
        provider: "?".to_string(),
        model: "?".to_string(),
        latency_ms: 0,
        sample: None,
        error: Some(format!("falha interna: {erro}")),
    })
}

#[tauri::command]
fn gpu_info() -> Option<c2_core::gpu::GpuInfo> {
    c2_core::gpu::detect()
}

#[derive(serde::Serialize)]
struct CostEstimate {
    pending: usize,
    estimated_cost_usd: f64,
    cap_usd: f64,
}

/// Estimativa de custo do proximo reindex (antes de chamar a nuvem).
#[tauri::command]
fn cost_estimate(root: String, full: bool) -> Result<CostEstimate, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    let items = c2_core::indexer::plan(&workspace, &config, full, None)
        .map_err(|err| err.to_string())?;
    let pending = items
        .iter()
        .filter(|item| {
            matches!(
                item.decision,
                c2_core::indexer::Decision::New | c2_core::indexer::Decision::Changed
            )
        })
        .count();
    Ok(CostEstimate {
        pending,
        estimated_cost_usd: c2_core::cost::estimar_plano(&config, &items),
        cap_usd: config.model.max_cost_usd_per_run,
    })
}

#[derive(serde::Serialize)]
struct CleanupPreview {
    found: usize,
}

#[derive(serde::Serialize)]
struct CleanupResult {
    found: usize,
    removed: usize,
    failed: Vec<String>,
}

#[tauri::command]
fn cleaner_plan(root: String) -> Result<CleanupPreview, String> {
    let workspace = open_workspace(&root)?;
    let report =
        c2_core::cleaner::clean(workspace.root(), true).map_err(|err| err.to_string())?;
    Ok(CleanupPreview {
        found: report.found.len(),
    })
}

#[tauri::command]
fn cleaner_run(root: String) -> Result<CleanupResult, String> {
    let workspace = open_workspace(&root)?;
    let report =
        c2_core::cleaner::clean(workspace.root(), false).map_err(|err| err.to_string())?;
    Ok(CleanupResult {
        found: report.found.len(),
        removed: report.removed.len(),
        failed: report.failed,
    })
}

#[tauri::command]
fn open_project_folder(root: String) -> Result<(), String> {
    let workspace = open_workspace(&root)?;
    let caminho = workspace.root().to_path_buf();
    #[cfg(target_os = "windows")]
    let mut comando = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut comando = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut comando = std::process::Command::new("xdg-open");
    comando
        .arg(&caminho)
        .spawn()
        .map(|_| ())
        .map_err(|erro| format!("nao foi possivel abrir {}: {erro}", caminho.display()))
}

#[tauri::command]
fn perf_log(app: tauri::AppHandle, event: String, data: Option<serde_json::Value>) {
    perf::registrar(&app, &event, data.unwrap_or_else(|| serde_json::json!({})));
}

#[tauri::command]
fn startup_elapsed_ms() -> u64 {
    perf::elapsed_ms()
}

#[tauri::command]
fn frontend_log(message: String) {
    eprintln!("[frontend] {message}");
}

#[tauri::command]
fn language_load(app: tauri::AppHandle) -> Option<String> {
    let caminho = app
        .path()
        .app_data_dir()
        .ok()?
        .join("language.json");
    let texto = std::fs::read_to_string(caminho).ok()?;
    let valor: serde_json::Value = serde_json::from_str(&texto).ok()?;
    valor.get("lang").and_then(|v| v.as_str()).map(str::to_string)
}

#[tauri::command]
fn language_save(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|erro| erro.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|erro| erro.to_string())?;
    let json = serde_json::json!({ "lang": lang });
    std::fs::write(
        dir.join("language.json"),
        serde_json::to_string_pretty(&json).map_err(|erro| erro.to_string())?,
    )
    .map_err(|erro| erro.to_string())
}

#[tauri::command]
fn news_fetch(url: String) -> Result<String, String> {
    c2_core::news::fetch(&url).map_err(|erro| erro.to_string())
}

/// URL de novidades sobrescrita no dev/teste (C2_NEWS_URL).
#[tauri::command]
fn news_url() -> Option<String> {
    std::env::var("C2_NEWS_URL")
        .ok()
        .map(|valor| valor.trim().to_string())
        .filter(|valor| !valor.is_empty())
}

fn news_state_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("news-state.json"))
        .map_err(|erro| erro.to_string())
}

#[tauri::command]
fn news_state_load(app: tauri::AppHandle) -> Option<String> {
    let caminho = news_state_path(&app).ok()?;
    std::fs::read_to_string(caminho).ok()
}

#[tauri::command]
fn news_state_save(app: tauri::AppHandle, state: String) -> Result<(), String> {
    let caminho = news_state_path(&app)?;
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai).map_err(|erro| erro.to_string())?;
    }
    let temporario = caminho.with_extension("json.tmp");
    std::fs::write(&temporario, state).map_err(|erro| erro.to_string())?;
    std::fs::rename(&temporario, &caminho).map_err(|erro| erro.to_string())
}

#[tauri::command]
fn prompt_sync(root: String, lang: String) -> Result<bool, String> {
    let workspace = open_workspace(&root)?;
    let mut config = workspace.load_config().map_err(|err| err.to_string())?;
    if !c2_core::config::is_builtin_prompt(&config.index.prompt) {
        return Ok(false);
    }
    let alvo = c2_core::config::default_prompt_for(&lang).to_string();
    if config.index.prompt == alvo {
        return Ok(false);
    }
    config.index.prompt = alvo;
    workspace
        .save_config(&config)
        .map_err(|err| err.to_string())?;
    Ok(true)
}

#[tauri::command]
fn prompt_default(lang: Option<String>) -> String {
    if lang.as_deref() == Some("pt-BR") {
        c2_core::config::DEFAULT_PROMPT_PT.to_string()
    } else {
        c2_core::config::DEFAULT_PROMPT.to_string()
    }
}

#[tauri::command]
fn pick_model(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .add_filter("Modelo GGUF", &["gguf"])
        .blocking_pick_file()
        .map(|path| path.to_string())
}

/// Ponto de entrada chamado por `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    perf::marcar_inicio();
    eprintln!("[c2] run: iniciando");
    // `c2 open --root <dir>` passa o projeto para a janela.
    let args: Vec<String> = std::env::args().collect();
    let root = args
        .iter()
        .position(|arg| arg == "--root")
        .and_then(|index| args.get(index + 1))
        .cloned();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Engine::default())
        .manage(Reindex::default())
        .manage(LastWorkspace::default())
        .manage(InitialRoot(root))
        .invoke_handler(tauri::generate_handler![
            initial_root,
            workspace_status,
            workspace_init,
            workspace_reindex,
            reindex_cancel,
            engine_start,
            engine_stop,
            engine_running,
            model_unload,
            search_grep,
            file_read,
            file_related,
            list_files,
            read_meta,
            config_get,
            config_set,
            pick_folder,
            pick_model,
            model_inspect,
            model_set,
            prompt_sync,
            prompt_default,
            perf_log,
            startup_elapsed_ms,
            cleaner_plan,
            cleaner_run,
            open_project_folder,
            frontend_log,
            language_load,
            language_save,
            news_fetch,
            news_url,
            news_state_load,
            news_state_save,
            gpu_info,
            mcp_smoke,
            api_key_set,
            api_key_delete,
            api_key_status,
            provider_test,
            cost_estimate,
            runtime_plan,
            model_load
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Fechar a janela nao encerra o app: esconde para a bandeja e
                // mantem o engine/indexacao vivos. Para sair, use o menu Sair.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            perf::registrar(app.handle(), "tauri_setup", serde_json::json!({}));

            let pt = app
                .path()
                .app_data_dir()
                .ok()
                .map(|dir| dir.join("language.json"))
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|texto| serde_json::from_str::<serde_json::Value>(&texto).ok())
                .and_then(|json| json.get("lang").and_then(|v| v.as_str()).map(|s| s == "pt-BR"))
                .unwrap_or(false);
            let (abrir_label, sair_label) = if pt {
                ("Abrir C2", "Sair")
            } else {
                ("Open C2", "Quit")
            };
            let abrir = MenuItem::with_id(app, "mostrar", abrir_label, true, None::<&str>)?;
            let sair = MenuItem::with_id(app, "sair", sair_label, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&abrir, &sair])?;

            if let Some(icone) = app.default_window_icon().cloned() {
                TrayIconBuilder::new()
                    .icon(icone)
                    .tooltip("C2 — Comments 2.0 for AI")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "mostrar" => {
                            if let Some(janela) = app.get_webview_window("main") {
                                let _ = janela.show();
                                let _ = janela.set_focus();
                            }
                        }
                        "sair" => {
                            if let Some(raiz) = app
                                .state::<LastWorkspace>()
                                .0
                                .lock()
                                .ok()
                                .and_then(|guard| guard.clone())
                            {
                                if let Ok(workspace) = open_workspace(&raiz) {
                                    let _ = runtime::stop(&workspace);
                                }
                            }
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            if let Some(janela) = tray.app_handle().get_webview_window("main") {
                                if janela.is_visible().unwrap_or(false) {
                                    let _ = janela.hide();
                                } else {
                                    let _ = janela.show();
                                    let _ = janela.set_focus();
                                }
                            }
                        }
                    })
                    .build(app)?;
            }

            eprintln!("[c2] setup: janela e bandeja prontas");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o C2 Desktop");
}
