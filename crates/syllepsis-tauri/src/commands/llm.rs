//! Commands for the optional LLM features: status, proposal generation, and acceptance.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use syllepsis_core::app::dto::NoteDto;
use syllepsis_core::app::llm::{
    self as app, CloudLlmCompletion, CloudLlmPrompt, LlmRouteStatus, LlmStatus,
};
use syllepsis_core::config::ModelRef;
use syllepsis_core::llm::{LlmTask, Proposal};
use syllepsis_core::onnx::{self, ModelCache, ModelManifest};

use crate::state::{models_root_from_app_data, AppState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDownloadFileReport {
    pub file_name: String,
    pub integrity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDownloadReport {
    pub model_id: String,
    pub downloaded_files: Vec<ModelDownloadFileReport>,
}

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// Current LLM provider/config status for the management view.
#[tauri::command]
pub fn llm_status(state: State<AppState>) -> Result<LlmStatus, String> {
    with_book!(state, book, {
        state.with_llm_service(book, |service| {
            Ok(LlmStatus {
                provider: service.provider_name().to_string(),
                live: service.is_live(),
                enabled: book.config.llm.enabled,
                auto_accept: book.config.llm.auto_accept,
            })
        })
    })
}

/// Effective provider/model route for every LLM task.
#[tauri::command]
pub fn llm_route_statuses(state: State<AppState>) -> Result<Vec<LlmRouteStatus>, String> {
    with_book!(state, book, { Ok(app::llm_route_statuses(book)) })
}

/// Generate (but do not apply) a proposal for a note and task.
#[tauri::command]
pub fn generate_proposal(
    state: State<AppState>,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> Result<Proposal, String> {
    with_book!(state, book, {
        state.with_llm_service(book, |service| {
            app::generate_proposal_with_service(book, service, &note_id, task, model_override)
                .map_err(|e| e.to_string())
        })
    })
}

/// Prepare a routed prompt for a frontend-owned cloud provider call.
#[tauri::command]
pub fn prepare_cloud_prompt(
    state: State<AppState>,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> Result<CloudLlmPrompt, String> {
    with_book!(state, book, {
        app::prepare_cloud_prompt(book, &note_id, task, model_override).map_err(|e| e.to_string())
    })
}

/// Wrap frontend cloud output into the shared proposal/acceptance flow.
#[tauri::command]
pub fn proposal_from_cloud_completion(
    state: State<AppState>,
    completion: CloudLlmCompletion,
) -> Result<Proposal, String> {
    with_book!(state, book, {
        app::proposal_from_cloud_completion(book, completion).map_err(|e| e.to_string())
    })
}

/// Apply a proposal to its target note.
#[tauri::command]
pub fn accept_proposal(
    state: State<AppState>,
    proposal: Proposal,
    store_old_as_commentary: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::accept_proposal(book, &proposal, store_old_as_commentary).map_err(|e| e.to_string())
    })
}

/// Built-in downloadable model manifests known by this app build.
#[tauri::command]
pub fn builtin_model_manifests() -> Vec<ModelManifest> {
    onnx::builtin_manifests()
}

/// Download any missing files for a built-in model into the machine-local app-data cache.
#[tauri::command]
pub fn download_builtin_model(
    app: AppHandle,
    state: State<AppState>,
    model_id: String,
) -> Result<ModelDownloadReport, String> {
    let manifest =
        onnx::builtin(&model_id).ok_or_else(|| format!("unknown model id: {model_id}"))?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let cache = ModelCache::new(models_root_from_app_data(&app_data_dir));
    let fetcher = onnx::HttpModelFetcher::new().map_err(|e| e.to_string())?;
    let downloaded = onnx::download_missing(&cache, &manifest, &fetcher)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(file_name, integrity)| ModelDownloadFileReport {
            file_name,
            integrity: format!("{integrity:?}"),
        })
        .collect();
    state.invalidate_llm_service();
    Ok(ModelDownloadReport {
        model_id,
        downloaded_files: downloaded,
    })
}
