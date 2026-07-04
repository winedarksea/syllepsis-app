//! Import-split LLM job queue: run [`LlmTask::ImportSplit`] over text-import chunks.
//!
//! Chunks are not stored notes, so jobs wrap the chunk text in a transient note and go through
//! the inline-note payload path (local queue or cloud shell execution). Acceptance is entirely
//! client-side: the UI compares proposed notes against the deterministic preview per chunk and
//! commits whichever side wins through the unchanged `commit_text_import`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use ulid::Ulid;

use syllepsis_core::app::llm::QueuedLlmJobStatus;
use syllepsis_core::config::ModelRef;
use syllepsis_core::llm::prompts::LlmTaskOptions;
use syllepsis_core::llm::service::{parse_import_split_response, ProposedImportNote};
use syllepsis_core::llm::{LlmTask, Proposal};
use syllepsis_core::model::{Note, ObjectType};

use crate::commands::cloud_llm;
use crate::commands::llm::{effective_llm_execution, EffectiveLlmExecution};
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportLlmChunkRequest {
    pub chunk_index: usize,
    #[serde(default)]
    pub heading: Option<String>,
    pub text: String,
    #[serde(default)]
    pub model_override: Option<ModelRef>,
    #[serde(default)]
    pub prompt_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportLlmJobResult {
    pub job_id: String,
    pub chunk_index: usize,
    pub status: QueuedLlmJobStatus,
    /// Parsed proposals. Empty with `error` set means the model replied but the reply did not
    /// parse — the UI keeps the deterministic side and shows `raw_output`.
    pub proposed: Vec<ProposedImportNote>,
    pub raw_output: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn enqueue_import_llm_chunk(
    app_handle: AppHandle,
    request: ImportLlmChunkRequest,
) -> Result<ImportLlmJobResult, String> {
    let state = app_handle.state::<AppState>();
    let job_id = Ulid::new().to_string();
    let initial = ImportLlmJobResult {
        job_id: job_id.clone(),
        chunk_index: request.chunk_index,
        status: QueuedLlmJobStatus::Queued,
        proposed: Vec::new(),
        raw_output: None,
        error: None,
    };
    state
        .import_llm_jobs
        .lock()
        .unwrap()
        .insert(job_id.clone(), initial.clone());

    tauri::async_runtime::spawn(run_import_llm_job(app_handle.clone(), job_id, request));

    Ok(initial)
}

#[tauri::command]
pub fn get_import_llm_job(
    state: State<AppState>,
    job_id: String,
) -> Result<Option<ImportLlmJobResult>, String> {
    Ok(state.import_llm_jobs.lock().unwrap().get(&job_id).cloned())
}

#[tauri::command]
pub fn list_import_llm_jobs(state: State<AppState>) -> Vec<ImportLlmJobResult> {
    let mut jobs: Vec<ImportLlmJobResult> = state
        .import_llm_jobs
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect();
    jobs.sort_by(|a, b| a.job_id.cmp(&b.job_id));
    jobs
}

#[tauri::command]
pub fn clear_import_llm_jobs(state: State<AppState>) {
    state.import_llm_jobs.lock().unwrap().clear();
}

async fn run_import_llm_job(app_handle: AppHandle, job_id: String, request: ImportLlmChunkRequest) {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        update_job(&state, &job_id, |job| {
            job.status = QueuedLlmJobStatus::Running;
        });
        match run_import_llm_job_inner(&state, &request) {
            Ok(proposal) => match parse_import_split_response(&proposal.content) {
                Ok(proposed) => update_job(&state, &job_id, |job| {
                    job.status = QueuedLlmJobStatus::Complete;
                    job.proposed = proposed;
                    job.raw_output = Some(proposal.content.clone());
                    job.error = None;
                }),
                // The model answered but the reply did not parse: complete with the raw output
                // visible so the user can fall back to the deterministic items.
                Err(parse_error) => update_job(&state, &job_id, |job| {
                    job.status = QueuedLlmJobStatus::Complete;
                    job.raw_output = Some(proposal.content.clone());
                    job.error = Some(parse_error);
                }),
            },
            Err(error) => update_job(&state, &job_id, |job| {
                job.status = QueuedLlmJobStatus::Failed;
                job.error = Some(error);
            }),
        }
    })
    .await;
    if let Err(error) = result {
        eprintln!("import LLM job task failed: {error}");
    }
}

fn run_import_llm_job_inner(
    state: &AppState,
    request: &ImportLlmChunkRequest,
) -> Result<Proposal, String> {
    let options = LlmTaskOptions {
        style_overrides: request
            .prompt_override
            .clone()
            .filter(|text| !text.trim().is_empty()),
        ..Default::default()
    };
    let note = transient_chunk_note(state, request)?;
    match effective_llm_execution(state, LlmTask::ImportSplit, request.model_override.clone())? {
        EffectiveLlmExecution::Cloud { model_ref } => {
            cloud_llm::generate_cloud_proposal_for_inline_note(
                state,
                &note,
                LlmTask::ImportSplit,
                Some(model_ref),
                &options,
            )
        }
        EffectiveLlmExecution::Local { model_ref } => {
            let (book_root, models_root) = {
                let guard = state.book.lock().unwrap();
                let book = guard
                    .as_ref()
                    .ok_or_else(|| "no book is open".to_string())?;
                let models_root = book
                    .models_root()
                    .ok_or_else(|| "local model directory unavailable".to_string())?
                    .to_path_buf();
                (book.root.clone(), models_root)
            };
            state.local_ai.submit_llm_inline_path(
                book_root,
                models_root,
                note,
                LlmTask::ImportSplit,
                Some(model_ref),
                options,
            )
        }
    }
}

/// Wrap the chunk text in a note that exists only for prompt construction.
fn transient_chunk_note(state: &AppState, request: &ImportLlmChunkRequest) -> Result<Note, String> {
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    let title = request
        .heading
        .clone()
        .filter(|heading| !heading.trim().is_empty())
        .unwrap_or_else(|| format!("Import chunk {}", request.chunk_index + 1));
    let mut note = Note::new(
        ObjectType::Note,
        title.trim(),
        book.config.markdown.dialect_version.clone(),
    );
    note.body = request.text.clone();
    Ok(note)
}

fn update_job(state: &AppState, job_id: &str, apply: impl FnOnce(&mut ImportLlmJobResult)) {
    if let Some(job) = state.import_llm_jobs.lock().unwrap().get_mut(job_id) {
        apply(job);
    }
}
