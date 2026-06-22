//! Prompt templates: turning a note + task into the `(system, user)` pair of an [`LlmRequest`].
//!
//! Each task has a system prompt that fixes the model's role and—crucially—its *output
//! contract* (e.g. category suggestions must come back as a comma-separated list so the service
//! can parse them deterministically regardless of which provider answered). Keeping the wording
//! here, named and in one file, is the config-driven alternative to scattering prompt strings
//! through the logic.

use crate::llm::task::LlmTask;
use crate::markdown::dialect::strip_comments;
use crate::model::Note;

/// System prompt per task (role + output contract).
fn system_prompt(task: LlmTask) -> &'static str {
    match task {
        LlmTask::Summarize => {
            "You are a concise editor. Summarize the note in one or two plain sentences \
             capturing its single main point. Output only the summary."
        }
        LlmTask::FactCheck => {
            "You are a careful fact-checker. Identify any claims in the note that are dubious, \
             unsupported, or incorrect, and briefly say why. If nothing is questionable, say so."
        }
        LlmTask::DevilsAdvocate => {
            "You are a sharp devil's advocate. Give the strongest good-faith counter-argument \
             to the note's position. Be specific and fair."
        }
        LlmTask::Grammar => {
            "You are a copy editor. Return the note's text with grammar, spelling, and clarity \
             fixed, preserving meaning, voice, and any #tags or @refs. Output only the revised text."
        }
        LlmTask::CategorySuggest => {
            "You suggest topical categories for a note. Output a comma-separated list of 1-5 \
             short lowercase tag names (no spaces, no # prefix) and nothing else."
        }
        LlmTask::Rewrite => {
            "You are a skilled writer. Rewrite the note's body to read clearly and well while \
             preserving its substance and any #tags or @refs. Output only the rewritten text."
        }
    }
}

/// The note text the model reasons over: title, summary, and body with comments removed.
fn note_text(note: &Note) -> String {
    let mut parts = Vec::new();
    if !note.title.trim().is_empty() {
        parts.push(format!("Title: {}", note.title));
    }
    if !note.summary.trim().is_empty() {
        parts.push(format!("Summary: {}", note.summary));
    }
    let body = strip_comments(&note.body);
    if !body.trim().is_empty() {
        parts.push(body.trim().to_string());
    }
    parts.join("\n\n")
}

/// Build the `(system, user)` prompt pair for `task` over `note`. For category suggestion the
/// book's existing categories are offered so the model can reuse them instead of inventing
/// near-duplicates.
pub fn build(task: LlmTask, note: &Note, known_categories: &[String]) -> (String, String) {
    let system = system_prompt(task).to_string();
    let mut user = note_text(note);

    if task == LlmTask::CategorySuggest && !known_categories.is_empty() {
        user.push_str("\n\nExisting categories to prefer when relevant: ");
        user.push_str(&known_categories.join(", "));
    }

    (system, user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    fn note() -> Note {
        let mut n = Note::new(ObjectType::Note, "Breaker safety", "syllepsis_001");
        n.summary = "Turn off power first".into();
        n.body = "Always switch off the breaker %%hidden%% before working.".into();
        n
    }

    #[test]
    fn includes_title_summary_and_strips_comments() {
        let (_sys, user) = build(LlmTask::Summarize, &note(), &[]);
        assert!(user.contains("Breaker safety"));
        assert!(user.contains("Turn off power first"));
        assert!(user.contains("switch off the breaker"));
        assert!(
            !user.contains("hidden"),
            "comments must be stripped from prompts"
        );
    }

    #[test]
    fn category_suggest_offers_existing_categories() {
        let cats = vec!["electrical".to_string(), "safety".to_string()];
        let (sys, user) = build(LlmTask::CategorySuggest, &note(), &cats);
        assert!(sys.contains("comma-separated"));
        assert!(user.contains("electrical, safety"));
    }
}
