//! The [`LlmProvider`] seam: anything that can answer a prompt.
//!
//! Exactly like [`crate::embeddings::EmbeddingProvider`], this is the one boundary the rest of
//! the app talks to. The built-in [`OfflineLlmProvider`](super::offline::OfflineLlmProvider)
//! implements it with deterministic, no-network heuristics so every LLM *flow* works and is
//! testable offline; a real provider (an Anthropic/Claude HTTP client, or a local model) is a
//! second `impl LlmProvider` added later — the router, prompts, service, and UI never change.

use serde::{Deserialize, Serialize};

use crate::config::ModelRef;
use crate::error::CoreResult;
use crate::llm::task::LlmTask;

/// A single, stateless completion request. `system` frames the role/output contract; `user`
/// carries the task input (usually the note's text wrapped by a prompt template). `task` is
/// included so a provider can branch on it; `model_ref` records the routed provider/model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequest {
    pub task: LlmTask,
    pub model_ref: ModelRef,
    pub system: String,
    pub user: String,
}

/// A provider's reply. Kept minimal (text only) so the boundary is trivial across providers;
/// task-specific structure (e.g. a category list) is parsed from `text` by the service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
}

/// Answers prompts. Implementations must not panic on bad input — return [`CoreError::Llm`]
/// instead so the UI can surface a message and the note is never left half-modified.
pub trait LlmProvider: Send {
    /// Short identifier shown in diagnostics / the management UI (e.g. `offline`, `anthropic`).
    fn name(&self) -> &str;

    /// Whether this provider performs real model inference (vs. the offline heuristic). The UI
    /// uses it to label generated content honestly.
    fn is_live(&self) -> bool {
        true
    }

    /// Complete one request.
    fn complete(&self, request: &LlmRequest) -> CoreResult<LlmResponse>;
}
