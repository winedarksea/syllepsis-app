//! Device-local controls and observability for the serial model worker.

use serde::Serialize;
use tauri::State;

use syllepsis_core::app::search as search_app;

use crate::local_ai::{LocalAiDevicePolicy, LocalAiWorkerStatus};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct LocalAiStatus {
    pub worker: LocalAiWorkerStatus,
    pub embedding_coverage: syllepsis_core::embeddings::EmbeddingCoverage,
}

#[tauri::command]
pub fn local_ai_status(state: State<AppState>) -> Result<LocalAiStatus, String> {
    let mut coverage = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        search_app::embedding_coverage(book).map_err(|error| error.to_string())?
    };
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
pub fn note_editing_finished(state: State<AppState>, note_id: String) -> Result<(), String> {
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    state.local_ai.enqueue_note(book, note_id, true)
}
