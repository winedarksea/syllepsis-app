//! A [`Proposal`]: one LLM-generated suggestion awaiting the human's accept/reject decision.
//!
//! Generation and acceptance are deliberately separate steps (object-types.md / the commentary
//! flow): the model proposes, the user disposes. A proposal carries everything the UI needs to
//! show the suggestion and everything the app layer needs to apply it — which note it targets,
//! which task and model produced it, and the generated text. Whether accepting replaces the
//! body or attaches an annotation is decided by [`LlmTask::replaces_body`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::config::ModelRef;
use crate::id::NoteId;
use crate::llm::task::LlmTask;

/// Lifecycle of a proposal. Proposals are generated on demand and not persisted in this phase,
/// so the status mainly distinguishes a fresh suggestion from one the user has acted on in the
/// current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
}

/// An LLM suggestion for a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// Opaque id for this suggestion (so the UI can track accept/reject of a specific one).
    pub id: String,
    pub target: NoteId,
    pub task: LlmTask,
    /// Provider that produced it (`local`, `offline`, `anthropic`, etc.).
    #[serde(default)]
    pub provider: String,
    /// The model that produced it (or `offline`), shown so generated content is labeled.
    pub model: String,
    /// Whether the model actually ran inference (false = offline heuristic).
    pub live: bool,
    pub content: String,
    pub status: ProposalStatus,
    pub created_at: DateTime<Utc>,
}

impl Proposal {
    /// A fresh, pending proposal with a minted id and current timestamp.
    pub fn new(
        target: NoteId,
        task: LlmTask,
        model_ref: ModelRef,
        content: impl Into<String>,
        live: bool,
    ) -> Proposal {
        Proposal {
            id: Ulid::new().to_string(),
            target,
            task,
            provider: model_ref.provider,
            model: model_ref.model,
            live,
            content: content.into(),
            status: ProposalStatus::Pending,
            created_at: Utc::now(),
        }
    }
}
