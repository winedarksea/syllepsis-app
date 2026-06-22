//! The set of LLM-assisted tasks Syllepsis routes per the model-router pattern.
//!
//! Each [`LlmTask`] is one job a model can do on a note. The task drives three things: which
//! provider/model it routes to ([`crate::config::LlmRouting`]), which prompt template builds the request
//! ([`super::prompts`]), and how the response is interpreted ([`super::service`]). Keeping the
//! taxonomy in one small enum means adding a task is a single, well-typed change.

use serde::{Deserialize, Serialize};

use crate::config::{LlmRouting, ModelRef};

/// One LLM-assisted operation on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmTask {
    /// Condense the note into a short summary (the flashcard front / chapter blurb).
    Summarize,
    /// Check the note's claims and flag anything dubious or unsupported.
    FactCheck,
    /// Argue the strongest counter-case to the note (the devil's advocate).
    DevilsAdvocate,
    /// Fix grammar/spelling/clarity without changing meaning.
    Grammar,
    /// Suggest categories (`#tags`) the note should belong to.
    CategorySuggest,
    /// Rewrite the body (e.g. toward a target style) while preserving substance.
    Rewrite,
}

impl LlmTask {
    /// Stable snake_case identifier (matches the serde representation).
    pub fn as_str(self) -> &'static str {
        match self {
            LlmTask::Summarize => "summarize",
            LlmTask::FactCheck => "fact_check",
            LlmTask::DevilsAdvocate => "devils_advocate",
            LlmTask::Grammar => "grammar",
            LlmTask::CategorySuggest => "category_suggest",
            LlmTask::Rewrite => "rewrite",
        }
    }

    /// The configured provider/model this task routes to.
    pub fn model_ref(self, routing: &LlmRouting) -> &ModelRef {
        match self {
            LlmTask::Summarize => &routing.summarize,
            LlmTask::FactCheck => &routing.fact_check,
            LlmTask::DevilsAdvocate => &routing.devils_advocate,
            LlmTask::Grammar => &routing.grammar,
            LlmTask::CategorySuggest => &routing.category_suggest,
            LlmTask::Rewrite => &routing.rewrite,
        }
    }

    /// Whether accepting this task's proposal replaces the note body (Grammar/Rewrite) versus
    /// attaching an annotation alongside it (the rest).
    pub fn replaces_body(self) -> bool {
        matches!(self, LlmTask::Grammar | LlmTask::Rewrite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_each_task_to_its_model() {
        let routing = LlmRouting::default();
        assert_eq!(
            LlmTask::FactCheck.model_ref(&routing).model,
            crate::onnx::manifest::BUNDLED_LLM_ID
        );
        assert_eq!(
            LlmTask::Summarize.model_ref(&routing).provider,
            crate::llm::selection::LOCAL_PROVIDER
        );
        assert_eq!(
            LlmTask::Summarize.model_ref(&routing).model,
            crate::onnx::manifest::BUNDLED_LLM_ID
        );
    }

    #[test]
    fn body_replacing_tasks_are_grammar_and_rewrite() {
        assert!(LlmTask::Grammar.replaces_body());
        assert!(LlmTask::Rewrite.replaces_body());
        assert!(!LlmTask::FactCheck.replaces_body());
        assert!(!LlmTask::Summarize.replaces_body());
    }

    #[test]
    fn serde_uses_snake_case() {
        let json = serde_json::to_string(&LlmTask::DevilsAdvocate).unwrap();
        assert_eq!(json, "\"devils_advocate\"");
    }
}
