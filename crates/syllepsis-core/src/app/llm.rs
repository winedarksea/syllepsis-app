//! Application command surface for the LLM features: report status, generate a proposal for a
//! note, and apply (accept) a proposal.
//!
//! Generation and acceptance are separate so the human stays in the loop (the commentary flow).
//! How a proposal is applied depends on its task: a summary fills the summary field, a
//! grammar/rewrite replaces the body (optionally archiving the old text as a commentary note), a
//! category suggestion merges tags, and a fact-check / devil's-advocate result is attached as a
//! linked commentary note rather than altering the original.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::app::dto::NoteDto;
use crate::config::ModelRef;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::llm::prompts;
use crate::llm::selection::select_llm_provider;
use crate::llm::service::parse_category_list;
use crate::llm::{LlmService, LlmTask, Proposal};
use crate::model::{Note, ObjectType};
use crate::onnx::{manifest, ModelCache};
use crate::storage::{Book, NoteStore};

/// A snapshot of the LLM configuration for the management UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmStatus {
    /// Active provider name (e.g. `offline`).
    pub provider: String,
    /// Whether the active provider performs real inference.
    pub live: bool,
    /// Whether LLM features are enabled in config.
    pub enabled: bool,
    /// Whether proposals are auto-accepted.
    pub auto_accept: bool,
}

/// How the UI should execute a routed task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmExecutionMode {
    /// LLM features are disabled at the book level.
    Disabled,
    /// Use the in-process bundled ONNX provider.
    Local,
    /// Use the frontend cloud handoff (`prepare_cloud_prompt` → provider SDK → proposal wrap).
    Cloud,
    /// Use deterministic offline heuristics because no live provider is available.
    OfflineFallback,
}

/// Effective route for one LLM task, including local model cache availability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRouteStatus {
    pub task: LlmTask,
    pub provider: String,
    pub model: String,
    pub execution_mode: LlmExecutionMode,
    pub available: bool,
}

/// A prompt package for a frontend-owned cloud provider call. Rust still owns routing and prompt
/// construction so cloud SDK differences do not drift the task contracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudLlmPrompt {
    pub target_note_id: String,
    pub task: LlmTask,
    pub provider: String,
    pub model: String,
    pub system: String,
    pub user: String,
    pub output_contract: String,
}

/// A cloud completion returned to Rust for wrapping into the shared proposal/acceptance flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudLlmCompletion {
    pub target_note_id: String,
    pub task: LlmTask,
    pub provider: String,
    pub model: String,
    pub content: String,
}

/// Build the LLM service for this book, selecting the ONNX local model when it is configured and
/// cached, otherwise falling through to the offline heuristic provider.
fn service_for(book: &Book) -> LlmService {
    let provider = select_llm_provider(book.models_root(), &book.config.llm);
    LlmService::new(provider, book.config.llm.routing.clone())
}

/// Report the current LLM status.
pub fn llm_status(book: &Book) -> LlmStatus {
    let service = service_for(book);
    LlmStatus {
        provider: service.provider_name().to_string(),
        live: service.is_live(),
        enabled: book.config.llm.enabled,
        auto_accept: book.config.llm.auto_accept,
    }
}

/// Report the effective route for every supported task. This keeps the UI from guessing whether
/// an action should call local generation, cloud prompt handoff, or show the offline fallback.
pub fn llm_route_statuses(book: &Book) -> Vec<LlmRouteStatus> {
    LLM_TASKS
        .iter()
        .copied()
        .map(|task| {
            let model_ref = task.model_ref(&book.config.llm.routing).clone();
            let (execution_mode, available) = route_execution_mode(book, &model_ref);
            LlmRouteStatus {
                task,
                provider: model_ref.provider,
                model: model_ref.model,
                execution_mode,
                available,
            }
        })
        .collect()
}

/// Generate a proposal for `note_id` and `task`. Does not modify the note.
pub fn generate_proposal(book: &Book, note_id: &str, task: LlmTask) -> CoreResult<Proposal> {
    let service = service_for(book);
    generate_proposal_with_service(book, &service, note_id, task, None)
}

/// Prepare a routed prompt for a frontend-owned cloud call. Local/offline routes should use
/// [`generate_proposal`] so the built-in model and offline fallback remain centralized.
pub fn prepare_cloud_prompt(
    book: &Book,
    note_id: &str,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> CoreResult<CloudLlmPrompt> {
    let note = book.store.read_note(&NoteId::parse(note_id)?)?;
    let model_ref =
        model_override.unwrap_or_else(|| task.model_ref(&book.config.llm.routing).clone());
    reject_non_cloud_route(task, &model_ref)?;
    let known_categories = known_categories(book)?;
    let (system, user) = prompts::build(task, &note, &known_categories);

    Ok(CloudLlmPrompt {
        target_note_id: note_id.to_string(),
        task,
        provider: model_ref.provider,
        model: model_ref.model,
        system,
        user,
        output_contract: output_contract(task).to_string(),
    })
}

/// Wrap frontend cloud output into a normal proposal. This keeps acceptance, audit labels, and
/// commentary-note behavior identical across local and cloud providers.
pub fn proposal_from_cloud_completion(
    book: &Book,
    completion: CloudLlmCompletion,
) -> CoreResult<Proposal> {
    let target = NoteId::parse(&completion.target_note_id)?;
    book.store.read_note(&target)?;
    let model_ref = ModelRef::new(completion.provider, completion.model);
    reject_non_cloud_route(completion.task, &model_ref)?;
    Ok(Proposal::new(
        target,
        completion.task,
        model_ref,
        completion.content,
        true,
    ))
}

/// Generate a proposal using a caller-owned long-lived service. Tauri uses this path so the local
/// ONNX provider is loaded once and reused rather than rebuilt for every command.
pub fn generate_proposal_with_service(
    book: &Book,
    service: &LlmService,
    note_id: &str,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> CoreResult<Proposal> {
    let note = book.store.read_note(&NoteId::parse(note_id)?)?;
    let known_categories: Vec<String> = book
        .store
        .categories()?
        .into_iter()
        .map(|c| c.name)
        .collect();
    match model_override {
        Some(model_ref) => {
            service.generate_with_model_ref(task, model_ref, &note, &known_categories)
        }
        None => service.generate(task, &note, &known_categories),
    }
}

fn known_categories(book: &Book) -> CoreResult<Vec<String>> {
    Ok(book
        .store
        .categories()?
        .into_iter()
        .map(|c| c.name)
        .collect())
}

fn reject_non_cloud_route(task: LlmTask, model_ref: &ModelRef) -> CoreResult<()> {
    if model_ref.provider == crate::llm::selection::LOCAL_PROVIDER
        || model_ref.provider == "offline"
    {
        return Err(CoreError::Llm(format!(
            "task {} routes to provider {}, not a cloud provider",
            task.as_str(),
            model_ref.provider
        )));
    }
    Ok(())
}

const LLM_TASKS: [LlmTask; 6] = [
    LlmTask::Summarize,
    LlmTask::FactCheck,
    LlmTask::DevilsAdvocate,
    LlmTask::Grammar,
    LlmTask::CategorySuggest,
    LlmTask::Rewrite,
];

fn route_execution_mode(book: &Book, model_ref: &ModelRef) -> (LlmExecutionMode, bool) {
    if !book.config.llm.enabled {
        return (LlmExecutionMode::Disabled, false);
    }
    if model_ref.provider == crate::llm::selection::LOCAL_PROVIDER {
        if local_model_is_cached(book, &model_ref.model) {
            (LlmExecutionMode::Local, true)
        } else {
            (LlmExecutionMode::OfflineFallback, true)
        }
    } else if model_ref.provider == "offline" {
        (LlmExecutionMode::OfflineFallback, true)
    } else {
        (LlmExecutionMode::Cloud, true)
    }
}

fn local_model_is_cached(book: &Book, model_id: &str) -> bool {
    let Some(models_root) = book.models_root() else {
        return false;
    };
    let Some(model_manifest) = manifest::builtin(model_id) else {
        return false;
    };
    ModelCache::new(models_root).is_cached(&model_manifest)
}

fn output_contract(task: LlmTask) -> &'static str {
    match task {
        LlmTask::Summarize => "plain_text_summary",
        LlmTask::FactCheck => "plain_text_fact_check",
        LlmTask::DevilsAdvocate => "plain_text_counterargument",
        LlmTask::Grammar => "plain_text_revised_body",
        LlmTask::CategorySuggest => "comma_separated_categories",
        LlmTask::Rewrite => "plain_text_rewritten_body",
    }
}

/// Apply a proposal to its target note. `store_old_as_commentary` only applies to body-replacing
/// tasks: when set, the pre-edit body is preserved as a linked commentary note before the
/// rewrite lands. Returns the updated target note.
pub fn accept_proposal(
    book: &Book,
    proposal: &Proposal,
    store_old_as_commentary: bool,
) -> CoreResult<NoteDto> {
    let mut note = book.store.read_note(&proposal.target)?;

    match proposal.task {
        LlmTask::Summarize => {
            note.summary = proposal.content.clone();
        }
        LlmTask::Grammar | LlmTask::Rewrite => {
            if store_old_as_commentary && !note.body.trim().is_empty() {
                let title = format!("Previous version of {}", note.title);
                create_commentary(book, &note, &title, &note.body.clone())?;
            }
            note.body = proposal.content.clone();
            note.metadata.authorship.ai_generated = true;
        }
        LlmTask::CategorySuggest => {
            for tag in parse_category_list(&proposal.content) {
                if !note.categories.contains(&tag) {
                    note.categories.push(tag);
                }
            }
        }
        LlmTask::FactCheck | LlmTask::DevilsAdvocate => {
            let title = format!("{} on {}", proposal.task.as_str(), note.title);
            create_commentary(book, &note, &title, &proposal.content)?;
        }
    }

    note.metadata.dates.updated = Utc::now();
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Create a commentary note linked to `target` by an `@ulid` reference, inheriting its
/// categories so it surfaces nearby. Marked AI-generated.
fn create_commentary(book: &Book, target: &Note, title: &str, body: &str) -> CoreResult<Note> {
    let mut commentary = book.new_note(ObjectType::Commentary, title)?;
    commentary.body = format!("@{}\n\n{}", target.id.ulid(), body);
    commentary.categories = target.categories.clone();
    commentary.metadata.authorship.ai_generated = true;
    book.save_note(&commentary)?;
    Ok(commentary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, update_note};
    use crate::config::ModelRef;
    use crate::storage::Book;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    fn seed(book: &Book, body: &str) -> NoteDto {
        let mut n = create_note(book, ObjectType::Note, "Breaker safety", None).unwrap();
        n.body = body.into();
        update_note(book, n).unwrap()
    }

    #[test]
    fn status_reports_local_default_with_offline_fallback_when_model_absent() {
        let (_d, book) = book();
        let status = llm_status(&book);
        assert_eq!(status.provider, "offline");
        assert!(!status.live);
        assert!(status.enabled);
    }

    #[test]
    fn route_status_reports_offline_fallback_when_local_model_is_missing() {
        let (_d, book) = book();
        let routes = llm_route_statuses(&book);
        let summarize = routes
            .iter()
            .find(|route| route.task == LlmTask::Summarize)
            .unwrap();

        assert_eq!(summarize.provider, crate::llm::selection::LOCAL_PROVIDER);
        assert_eq!(summarize.execution_mode, LlmExecutionMode::OfflineFallback);
        assert!(summarize.available);
    }

    #[test]
    fn route_status_reports_cloud_for_cloud_routed_task() {
        let (_d, mut book) = book();
        book.config.llm.routing.fact_check = ModelRef::new("anthropic", "claude-opus");

        let routes = llm_route_statuses(&book);
        let fact_check = routes
            .iter()
            .find(|route| route.task == LlmTask::FactCheck)
            .unwrap();

        assert_eq!(fact_check.provider, "anthropic");
        assert_eq!(fact_check.model, "claude-opus");
        assert_eq!(fact_check.execution_mode, LlmExecutionMode::Cloud);
        assert!(fact_check.available);
    }

    #[test]
    fn route_status_reports_disabled_when_llm_is_disabled() {
        let (_d, mut book) = book();
        book.config.llm.enabled = false;

        let routes = llm_route_statuses(&book);

        assert!(routes
            .iter()
            .all(|route| route.execution_mode == LlmExecutionMode::Disabled));
        assert!(routes.iter().all(|route| !route.available));
    }

    #[test]
    fn accept_summary_sets_the_summary_field() {
        let (_d, book) = book();
        let note = seed(&book, "Turn the breaker off first. Then test.");
        let proposal = generate_proposal(&book, &note.id, LlmTask::Summarize).unwrap();
        let updated = accept_proposal(&book, &proposal, false).unwrap();
        assert!(!updated.summary.is_empty());
        assert_eq!(updated.summary, proposal.content);
    }

    #[test]
    fn accept_category_suggest_merges_tags() {
        let (_d, book) = book();
        let note = seed(&book, "breaker breaker electrical panel kitchen breaker");
        let proposal = generate_proposal(&book, &note.id, LlmTask::CategorySuggest).unwrap();
        let updated = accept_proposal(&book, &proposal, false).unwrap();
        assert!(updated.categories.contains(&"breaker".to_string()));
    }

    #[test]
    fn accept_fact_check_creates_linked_commentary() {
        let (_d, book) = book();
        let note = seed(&book, "Solar pays back in two years guaranteed.");
        let before = book.store.read_all_notes().unwrap().len();
        let proposal = generate_proposal(&book, &note.id, LlmTask::FactCheck).unwrap();
        accept_proposal(&book, &proposal, false).unwrap();

        let notes = book.store.read_all_notes().unwrap();
        assert_eq!(
            notes.len(),
            before + 1,
            "a commentary note should be created"
        );
        let commentary = notes
            .iter()
            .find(|n| n.object_type == ObjectType::Commentary)
            .unwrap();
        // Links back to the target by ulid reference.
        assert!(commentary
            .body
            .contains(note.id.split('-').next_back().unwrap()));
        assert!(commentary.metadata.authorship.ai_generated);
    }

    #[test]
    fn accept_rewrite_can_archive_the_old_body() {
        let (_d, book) = book();
        let note = seed(&book, "original body kept as history");
        let mut proposal = generate_proposal(&book, &note.id, LlmTask::Rewrite).unwrap();
        // Simulate a live rewrite result (offline would be a no-op).
        proposal.content = "a cleaner rewritten body".into();
        let updated = accept_proposal(&book, &proposal, true).unwrap();

        assert_eq!(updated.body, "a cleaner rewritten body");
        let archived = book
            .store
            .read_all_notes()
            .unwrap()
            .into_iter()
            .any(|n| n.body.contains("original body kept as history"));
        assert!(
            archived,
            "the previous body should be preserved as a commentary"
        );
    }

    #[test]
    fn prepare_cloud_prompt_uses_routed_provider_model_and_note_text() {
        let (_d, mut book) = book();
        book.config.llm.routing.fact_check = ModelRef::new("anthropic", "claude-opus");
        let note = seed(&book, "Solar pays back in two years guaranteed.");

        let prompt = prepare_cloud_prompt(&book, &note.id, LlmTask::FactCheck, None).unwrap();

        assert_eq!(prompt.provider, "anthropic");
        assert_eq!(prompt.model, "claude-opus");
        assert_eq!(prompt.output_contract, "plain_text_fact_check");
        assert!(prompt.user.contains("Solar pays back"));
        assert!(prompt.system.contains("fact-checker"));
    }

    #[test]
    fn prepare_cloud_prompt_rejects_local_route() {
        let (_d, book) = book();
        let note = seed(&book, "Use the local default.");
        let err = prepare_cloud_prompt(&book, &note.id, LlmTask::Summarize, None).unwrap_err();
        assert!(err.to_string().contains("not a cloud provider"));
    }

    #[test]
    fn prepare_cloud_prompt_accepts_per_action_model_override() {
        let (_d, book) = book();
        let note = seed(&book, "Use a stronger cloud model for this one check.");

        let prompt = prepare_cloud_prompt(
            &book,
            &note.id,
            LlmTask::FactCheck,
            Some(ModelRef::new("anthropic", "claude-sonnet")),
        )
        .unwrap();

        assert_eq!(prompt.provider, "anthropic");
        assert_eq!(prompt.model, "claude-sonnet");
        assert_eq!(prompt.task, LlmTask::FactCheck);
    }

    #[test]
    fn cloud_completion_wraps_as_live_provider_labeled_proposal() {
        let (_d, book) = book();
        let note = seed(&book, "Cloud provider returns this answer.");

        let proposal = proposal_from_cloud_completion(
            &book,
            CloudLlmCompletion {
                target_note_id: note.id.clone(),
                task: LlmTask::DevilsAdvocate,
                provider: "openai".into(),
                model: "gpt-4.1".into(),
                content: "A counterpoint.".into(),
            },
        )
        .unwrap();

        assert_eq!(proposal.target.to_string(), note.id);
        assert_eq!(proposal.provider, "openai");
        assert_eq!(proposal.model, "gpt-4.1");
        assert_eq!(proposal.content, "A counterpoint.");
        assert!(proposal.live);
    }
}
