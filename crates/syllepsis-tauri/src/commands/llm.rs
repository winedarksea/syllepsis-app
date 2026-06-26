//! Commands for the optional LLM features: status, proposal generation, and acceptance.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use ulid::Ulid;

use syllepsis_core::app::dto::NoteDto;
use syllepsis_core::app::llm::{
    self as app, CloudLlmCompletion, CloudLlmPrompt, LlmRouteStatus, LlmStatus,
    QueuedLlmJobRequest, QueuedLlmJobResult, QueuedLlmJobStatus,
};
use syllepsis_core::config::ModelRef;
use syllepsis_core::llm::prompts::{LlmTaskOptions, PromptStyleCard};
use syllepsis_core::llm::{LlmTask, Proposal};
use syllepsis_core::onnx::{self, FileIntegrity, ModelCache, ModelCacheStatus, ModelManifest};

use crate::commands::cloud_llm::cloud_provider_is_configured;
use crate::commands::{cloud_llm, style_cards};
use crate::state::{models_root_from_app_data, AppState, QueuedLlmJobRecord};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDownloadFileReport {
    pub file_name: String,
    pub integrity: FileIntegrity,
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
    with_book!(state, book, { Ok(app::llm_status(book)) })
}

/// Effective provider/model route for every LLM task.
#[tauri::command]
pub fn llm_route_statuses(state: State<AppState>) -> Result<Vec<LlmRouteStatus>, String> {
    with_book!(state, book, {
        app::llm_route_statuses(book)
            .into_iter()
            .map(|mut route| {
                if route.execution_mode == app::LlmExecutionMode::Cloud {
                    route.available = cloud_provider_is_configured(&route.provider)?;
                }
                Ok(route)
            })
            .collect()
    })
}

#[tauri::command]
pub fn enqueue_llm_job(
    app_handle: AppHandle,
    request: QueuedLlmJobRequest,
) -> Result<QueuedLlmJobResult, String> {
    let state = app_handle.state::<AppState>();
    let job_id = Ulid::new().to_string();
    let options = task_options_with_style_card(&state, &request)?;
    let initial = QueuedLlmJobResult {
        job_id: job_id.clone(),
        status: QueuedLlmJobStatus::Queued,
        target_note_id: request.target_note_id.clone(),
        task: request.task,
        proposal: None,
        error: None,
    };
    state.llm_jobs.lock().unwrap().insert(
        job_id.clone(),
        QueuedLlmJobRecord {
            result: initial.clone(),
            options: options.clone(),
            dismissed: false,
        },
    );

    tauri::async_runtime::spawn(run_queued_llm_job(
        app_handle,
        job_id,
        request,
        options,
    ));

    Ok(initial)
}

#[tauri::command]
pub fn list_llm_jobs(state: State<AppState>) -> Vec<QueuedLlmJobResult> {
    let mut jobs = state
        .llm_jobs
        .lock()
        .unwrap()
        .values()
        .filter(|record| !record.dismissed)
        .map(|record| record.result.clone())
        .collect::<Vec<_>>();
    jobs.sort_by(|a, b| a.job_id.cmp(&b.job_id));
    jobs
}

/// List all LLM jobs including dismissed ones, for the history panel.
#[tauri::command]
pub fn list_all_llm_jobs(state: State<AppState>) -> Vec<QueuedLlmJobResult> {
    let mut jobs = state
        .llm_jobs
        .lock()
        .unwrap()
        .values()
        .map(|record| record.result.clone())
        .collect::<Vec<_>>();
    jobs.sort_by(|a, b| b.job_id.cmp(&a.job_id));
    jobs
}

#[tauri::command]
pub fn get_llm_job(
    state: State<AppState>,
    job_id: String,
) -> Result<Option<QueuedLlmJobResult>, String> {
    Ok(state
        .llm_jobs
        .lock()
        .unwrap()
        .get(&job_id)
        .filter(|record| !record.dismissed)
        .map(|record| record.result.clone()))
}

#[tauri::command]
pub fn accept_llm_job_result(
    state: State<AppState>,
    job_id: String,
    store_old_as_commentary: bool,
    fact_check_passed: bool,
) -> Result<NoteDto, String> {
    let proposal = {
        let jobs = state.llm_jobs.lock().unwrap();
        let record = jobs
            .get(&job_id)
            .ok_or_else(|| format!("unknown LLM job: {job_id}"))?;
        if record.result.status != QueuedLlmJobStatus::Complete {
            return Err(format!("LLM job {job_id} is not complete"));
        }
        record
            .result
            .proposal
            .clone()
            .ok_or_else(|| format!("LLM job {job_id} has no proposal"))?
    };

    with_book!(state, book, {
        let updated = app::accept_proposal(
            book,
            &proposal,
            store_old_as_commentary,
            fact_check_passed,
        )
        .map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, updated.id.clone(), false);
        state.invalidate_graph_corpus();
        if let Some(record) = state.llm_jobs.lock().unwrap().get_mut(&job_id) {
            record.dismissed = true;
        }
        Ok(updated)
    })
}

#[tauri::command]
pub fn dismiss_llm_job_result(state: State<AppState>, job_id: String) -> Result<(), String> {
    let mut jobs = state.llm_jobs.lock().unwrap();
    let record = jobs
        .get_mut(&job_id)
        .ok_or_else(|| format!("unknown LLM job: {job_id}"))?;
    record.dismissed = true;
    Ok(())
}

fn task_options_with_style_card(
    state: &AppState,
    request: &QueuedLlmJobRequest,
) -> Result<LlmTaskOptions, String> {
    let mut options = request.task_options();
    let Some(style_card_id) = &request.style_card_id else {
        return Ok(options);
    };
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or_else(|| "no book is open".to_string())?;
    let Some(card) = style_cards::style_card_for_book(&book.root, style_card_id)? else {
        return Err(format!("style card not found: {style_card_id}"));
    };
    options.style_card = Some(PromptStyleCard {
        id: card.id,
        name: card.name,
        short_description: card.short_description,
        verbosity: card.verbosity,
        perspective: card.perspective,
        reading_level: card.reading_level,
        voice: card.voice,
        patterns: card.patterns.into_iter().map(|pattern| pattern.text).collect(),
        exemplars: card
            .exemplars
            .into_iter()
            .map(|exemplar| exemplar.text)
            .collect(),
    });
    Ok(options)
}

async fn run_queued_llm_job(
    app_handle: AppHandle,
    job_id: String,
    request: QueuedLlmJobRequest,
    options: LlmTaskOptions,
) {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        update_job_status(&state, &job_id, QueuedLlmJobStatus::Running, None, None);
        let proposal_result = run_queued_llm_job_inner(&state, &request, &options);
        match proposal_result {
            Ok(proposal) => {
                let commentary_error = if options.store_result_as_commentary {
                    let guard = state.book.lock().unwrap();
                    match guard.as_ref() {
                        Some(book) => app::create_proposal_commentary(
                            book,
                            &proposal,
                            &job_id,
                            &options,
                        )
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                        None => Err("no book is open".to_string()),
                    }
                } else {
                    Ok(())
                };
                match commentary_error {
                    Ok(()) => {
                        update_job_status(
                            &state,
                            &job_id,
                            QueuedLlmJobStatus::Complete,
                            Some(proposal),
                            None,
                        );
                    }
                    Err(error) => {
                        update_job_status(
                            &state,
                            &job_id,
                            QueuedLlmJobStatus::Failed,
                            Some(proposal),
                            Some(error),
                        );
                    }
                }
            }
            Err(error) => update_job_status(
                &state,
                &job_id,
                QueuedLlmJobStatus::Failed,
                None,
                Some(error),
            ),
        }
    })
    .await;
    if let Err(error) = result {
        eprintln!("queued LLM job task failed: {error}");
    }
}

fn run_queued_llm_job_inner(
    state: &AppState,
    request: &QueuedLlmJobRequest,
    options: &LlmTaskOptions,
) -> Result<Proposal, String> {
    let route = {
        let guard = state.book.lock().unwrap();
        let book = guard.as_ref().ok_or_else(|| "no book is open".to_string())?;
        app::llm_route_statuses(book)
            .into_iter()
            .find(|route| route.task == request.task)
            .ok_or_else(|| format!("no LLM route for {}", request.task.as_str()))?
    };
    if !route.available {
        return Err(format!(
            "No runnable LLM is configured for {} via {}/{}.",
            request.task.as_str(),
            route.provider,
            route.model
        ));
    }
    match route.execution_mode {
        app::LlmExecutionMode::Cloud => cloud_llm::generate_cloud_proposal_for_state(
            state,
            request.target_note_id.clone(),
            request.task,
            request.model_override.clone(),
            options,
        ),
        app::LlmExecutionMode::Local => {
            let (book_root, models_root) = {
                let guard = state.book.lock().unwrap();
                let book = guard.as_ref().ok_or_else(|| "no book is open".to_string())?;
                let models_root = book
                    .models_root()
                    .ok_or_else(|| "local model directory unavailable".to_string())?
                    .to_path_buf();
                (book.root.clone(), models_root)
            };
            state.local_ai.submit_llm_path(
                book_root,
                models_root,
                request.target_note_id.clone(),
                request.task,
                request.model_override.clone(),
                options.clone(),
            )
        }
        app::LlmExecutionMode::Disabled | app::LlmExecutionMode::Unavailable => {
            Err(format!("No runnable LLM is configured for {}.", request.task.as_str()))
        }
    }
}

fn update_job_status(
    state: &AppState,
    job_id: &str,
    status: QueuedLlmJobStatus,
    proposal: Option<Proposal>,
    error: Option<String>,
) {
    if let Some(record) = state.llm_jobs.lock().unwrap().get_mut(job_id) {
        record.result.status = status;
        if proposal.is_some() {
            record.result.proposal = proposal;
        }
        record.result.error = error;
    }
}

/// Generate (but do not apply) a proposal for a note and task.
#[tauri::command]
pub async fn generate_proposal(
    app: AppHandle,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> Result<Proposal, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        with_book!(state, book, {
            state
                .local_ai
                .submit_llm(book, note_id, task, model_override, LlmTaskOptions::default())
        })
    })
    .await
    .map_err(|error| format!("local LLM worker failed: {error}"))?
}

/// Prepare a routed prompt for shell-owned cloud/local-server execution.
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

/// Wrap external provider output into the shared proposal/acceptance flow.
#[tauri::command]
pub fn proposal_from_cloud_completion(
    state: State<AppState>,
    completion: CloudLlmCompletion,
) -> Result<Proposal, String> {
    with_book!(state, book, {
        app::proposal_from_cloud_completion(book, completion).map_err(|e| e.to_string())
    })
}

/// Apply a proposal to its target note. `fact_check_passed` satisfies a fact-check-gated lock.
#[tauri::command]
pub fn accept_proposal(
    state: State<AppState>,
    proposal: Proposal,
    store_old_as_commentary: bool,
    fact_check_passed: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated =
            app::accept_proposal(book, &proposal, store_old_as_commentary, fact_check_passed)
                .map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, updated.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Built-in downloadable model manifests known by this app build.
#[tauri::command]
pub fn builtin_model_manifests() -> Vec<ModelManifest> {
    onnx::builtin_manifests()
}

/// Inspect the machine-local cache status for every built-in model.
#[tauri::command]
pub fn builtin_model_cache_statuses(
    app: AppHandle,
    verify_hashes: bool,
) -> Result<Vec<ModelCacheStatus>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let cache = ModelCache::new(models_root_from_app_data(&app_data_dir));
    onnx::builtin_manifests()
        .iter()
        .map(|manifest| {
            onnx::inspect_model_cache(&cache, manifest, verify_hashes).map_err(|e| e.to_string())
        })
        .collect()
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
            integrity,
        })
        .collect();
    state.invalidate_llm_service();
    state.invalidate_graph_corpus();
    if manifest.kind == onnx::ModelKind::Embedding {
        if let Some(book) = state.book.lock().unwrap().as_ref() {
            let _ = state.local_ai.enqueue_all_stale(book, true);
        }
    }
    Ok(ModelDownloadReport {
        model_id,
        downloaded_files: downloaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_report_serializes_integrity_as_a_typed_enum() {
        let report = ModelDownloadFileReport {
            file_name: "model.onnx".to_string(),
            integrity: FileIntegrity::Verified,
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["integrity"], "verified");

        let mismatch_report = ModelDownloadFileReport {
            file_name: "model.onnx".to_string(),
            integrity: FileIntegrity::Mismatch {
                expected: "expected".to_string(),
                actual: "actual".to_string(),
            },
        };
        let mismatch_json = serde_json::to_value(mismatch_report).unwrap();
        assert_eq!(
            mismatch_json["integrity"]["mismatch"]["expected"],
            "expected"
        );
        assert_eq!(mismatch_json["integrity"]["mismatch"]["actual"], "actual");
    }
}
