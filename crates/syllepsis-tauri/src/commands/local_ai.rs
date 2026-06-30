//! Device-local controls and observability for the serial model worker.

use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use syllepsis_core::app::search as search_app;
use syllepsis_core::onnx::{self, ModelCache};
use syllepsis_core::storage::NoteStore;

use crate::local_ai::{LocalAiDevicePolicy, LocalAiWorkerStatus};
use crate::state::{models_root_from_app_data, AppState};

#[derive(Debug, Clone, Serialize)]
pub struct LocalAiStatus {
    pub worker: LocalAiWorkerStatus,
    pub embedding_coverage: syllepsis_core::embeddings::EmbeddingCoverage,
    pub embedding_model_id: String,
    pub embedding_model_cached: bool,
}

#[tauri::command]
pub fn local_ai_status(app: AppHandle, state: State<AppState>) -> Result<LocalAiStatus, String> {
    let (mut coverage, embedding_model_id) = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        (
            search_app::embedding_coverage(book).map_err(|error| error.to_string())?,
            book.config.embedding.model_id.clone(),
        )
    };
    let embedding_manifest = onnx::builtin(&embedding_model_id)
        .ok_or_else(|| format!("unknown embedding model {embedding_model_id}"))?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir: {error}"))?;
    let embedding_model_cached =
        ModelCache::new(models_root_from_app_data(&app_data_dir)).is_cached(&embedding_manifest);
    let worker = state.local_ai.status();
    let needs_embeddings =
        coverage.stale_notes + coverage.missing_notes + coverage.incompatible_notes;
    coverage.blocked_notes = worker.blocked_note_jobs.min(needs_embeddings);
    if !worker.policy.generate_note_embeddings
        || (worker.policy.pause_note_embeddings_on_battery
            && worker.power_source == crate::local_ai::PowerSource::Battery)
    {
        coverage.blocked_notes = needs_embeddings;
    }
    Ok(LocalAiStatus {
        worker,
        embedding_coverage: coverage,
        embedding_model_id,
        embedding_model_cached,
    })
}

#[tauri::command]
pub fn get_local_ai_device_policy(state: State<AppState>) -> LocalAiDevicePolicy {
    state.local_ai.policy()
}

#[tauri::command]
pub fn update_local_ai_device_policy(
    state: State<AppState>,
    policy: LocalAiDevicePolicy,
) -> Result<LocalAiDevicePolicy, String> {
    state.local_ai.update_policy(policy)
}

#[tauri::command]
pub fn enqueue_all_stale_embeddings(state: State<AppState>) -> Result<usize, String> {
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    state.local_ai.enqueue_all_stale(book, true)
}

#[tauri::command]
pub fn note_editing_finished(
    app: AppHandle,
    state: State<AppState>,
    note_id: String,
) -> Result<(), String> {
    {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        let id = syllepsis_core::id::NoteId::parse(&note_id).map_err(|error| error.to_string())?;
        let note = book
            .store
            .read_note(&id)
            .map_err(|error| error.to_string())?;
        if syllepsis_core::embeddings::note_embedding_is_stale(book, &note)
            .map_err(|error| error.to_string())?
        {
            state.local_ai.enqueue_note(book, note_id, true)?;
        }
    }

    // Finishing a note is a natural sync trigger, but rapid note exploration should not hammer
    // the cloud. Increment the generation counter so only the *last* editing-finished within the
    // debounce window actually syncs; earlier ones wake up, find a newer generation, and exit.
    let gen = state
        .sync_debounce_gen
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let debounce_gen = Arc::clone(&state.sync_debounce_gen);
    let sync_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if debounce_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        let state = sync_app.state::<AppState>();
        if let Err(error) =
            crate::commands::sync::sync_connected_managed_cloud_providers(&sync_app, &state)
        {
            tracing::debug!(error = %error, "note-editing-finished sync skipped");
        }
    });
    Ok(())
}
