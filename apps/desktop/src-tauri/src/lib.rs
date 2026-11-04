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
use tauri::{Emitter, State};
use tauri_plugin_dialog::DialogExt;

/// Projeto passado por `c2 open --root <dir>`.
struct InitialRoot(Option<String>);

/// Estado do laco de indexacao controlado pela janela.
#[derive(Default)]
struct Engine {
    running: Arc<AtomicBool>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

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

/// Projeto com que a janela foi aberta (se veio de `c2 open`).
#[tauri::command]
fn initial_root(initial: State<'_, InitialRoot>) -> Option<String> {
    initial.0.clone()
}

#[tauri::command]
fn workspace_status(root: String) -> Result<Option<StatusReport>, String> {
    let workspace = open_workspace(&root)?;
    if !workspace.is_initialized() {
        return Ok(None);
    }
    status::build(&workspace)
        .map(Some)
        .map_err(|err| err.to_string())
}

#[derive(Serialize)]
struct InitResult {
    created: bool,
    files: usize,
}

#[tauri::command]
fn workspace_init(root: String) -> Result<InitResult, String> {
    let path = c2_core::workspace::canonical_path(Path::new(&root)).map_err(|err| err.to_string())?;
    let workspace = Workspace::at(path);
    let outcome = workspace.init(false).map_err(|err| err.to_string())?;
    Ok(InitResult {
        created: outcome.created,
        files: outcome.files,
    })
}

/// Uma passada completa: sobe runtime, indexa, atualiza estado e para o runtime.
#[tauri::command]
async fn workspace_reindex(
    root: String,
    full: Option<bool>,
    limit: Option<usize>,
) -> Result<IndexReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = open_workspace(&root)?;
        let config = workspace.load_config().map_err(|err| err.to_string())?;
        require_model(&config)?;
        let client = LlmClient::from_config(&config);
        runtime::ensure(&workspace, &config).map_err(|err| err.to_string())?;

        let options = IndexOptions {
            full: full.unwrap_or(false),
            limit,
            only_path: None,
            dry_run: false,
        };
        let report =
            indexer::run(&workspace, &config, &client, &options, |_| {}).map_err(|err| err.to_string())?;

        let mut state = EngineState::load(&workspace.state_path()).map_err(|err| err.to_string())?;
        state.merge_report(&report);
        state
            .save(&workspace.state_path())
            .map_err(|err| err.to_string())?;

        let _ = runtime::stop(&workspace);
        Ok(report)
    })
    .await
    .map_err(|err| err.to_string())?
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
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    require_model(&config)?;
    let client = LlmClient::from_config(&config);
    let running = Arc::clone(&engine.running);
    running.store(true, Ordering::SeqCst);

    let handle = std::thread::spawn(move || {
        if let Err(err) = runtime::ensure(&workspace, &config) {
            let _ = app.emit("engine://error", err.to_string());
            running.store(false, Ordering::SeqCst);
            return;
        }

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
                    let _ = app.emit("engine://cycle", report);
                }
                Err(err) => {
                    let _ = app.emit("engine://error", err.to_string());
                }
            }

            // Watchdog: servidor caiu? sobe de novo (com backoff interno).
            if !runtime::health(&config) {
                let _ = runtime::ensure(&workspace, &config);
            }
            std::thread::sleep(interval);
        }

        let _ = runtime::stop(&workspace);
        let _ = app.emit("engine://stopped", ());
    });

    *engine.handle.lock().map_err(|err| err.to_string())? = Some(handle);
    Ok(())
}

#[tauri::command]
fn engine_stop(root: String, engine: State<'_, Engine>) -> Result<(), String> {
    engine.running.store(false, Ordering::SeqCst);
    if let Ok(workspace) = open_workspace(&root) {
        let _ = runtime::stop(&workspace);
    }
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
) -> Result<Vec<search::Hit>, String> {
    let workspace = open_workspace(&root)?;
    let config = workspace.load_config().map_err(|err| err.to_string())?;
    let options = SearchOptions {
        limit: limit.unwrap_or(10),
        include_snippets: true,
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
fn config_set(root: String, key: String, value: String) -> Result<String, String> {
    let workspace = open_workspace(&root)?;
    let mut config = workspace.load_config().map_err(|err| err.to_string())?;
    config.set(&key, &value).map_err(|err| err.to_string())?;
    workspace.save_config(&config).map_err(|err| err.to_string())?;
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
fn model_set(root: String, path: String) -> Result<ModelInspection, String> {
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
    Ok(inspection)
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
    // `c2 open --root <dir>` passa o projeto para a janela.
    let args: Vec<String> = std::env::args().collect();
    let root = args
        .iter()
        .position(|arg| arg == "--root")
        .and_then(|index| args.get(index + 1))
        .cloned();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Engine::default())
        .manage(InitialRoot(root))
        .invoke_handler(tauri::generate_handler![
            initial_root,
            workspace_status,
            workspace_init,
            workspace_reindex,
            engine_start,
            engine_stop,
            engine_running,
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
            model_set
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o C2 Desktop");
}
