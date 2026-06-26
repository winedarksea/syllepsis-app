//! Prompt templates: turning a note + task into the `(system, user)` pair of an [`LlmRequest`].
//!
//! Each task has a system prompt that fixes the model's role and—crucially—its *output
//! contract* (e.g. category suggestions must come back as a comma-separated list so the service
//! can parse them deterministically regardless of which provider answered). Keeping the wording
//! here, named and in one file, is the config-driven alternative to scattering prompt strings
//! through the logic.

use serde::{Deserialize, Serialize};

use crate::llm::task::LlmTask;
use crate::markdown::dialect::strip_comments;
use crate::model::Note;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryVariant {
    #[default]
    Plain,
    Mnemonic,
    Acrostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RewriteMode {
    #[default]
    Standard,
    Simplify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptStyleCard {
    pub id: String,
    pub name: String,
    pub short_description: String,
    pub verbosity: String,
    pub perspective: String,
    pub reading_level: String,
    pub voice: String,
    pub patterns: Vec<String>,
    pub exemplars: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LlmTaskOptions {
    pub style_card_id: Option<String>,
    pub style_card: Option<PromptStyleCard>,
    pub style_overrides: Option<String>,
    pub summary_variant: SummaryVariant,
    pub rewrite_mode: RewriteMode,
    pub store_result_as_commentary: bool,
}

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
        LlmTask::GenerateFromSummary => {
            "You are a skilled writer. Generate a complete note body from the note's title and \
             summary. Preserve any #tags or @refs already present. Output only the generated body."
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
    build_with_options(task, note, known_categories, &LlmTaskOptions::default())
}

pub fn build_with_options(
    task: LlmTask,
    note: &Note,
    known_categories: &[String],
    options: &LlmTaskOptions,
) -> (String, String) {
    let system = system_prompt(task).to_string();
    let mut user = if task == LlmTask::GenerateFromSummary {
        let mut parts = Vec::new();
        if !note.title.trim().is_empty() {
            parts.push(format!("Title: {}", note.title));
        }
        if !note.summary.trim().is_empty() {
            parts.push(format!("Summary: {}", note.summary));
        }
        parts.join("\n\n")
    } else {
        note_text(note)
    };

    if task == LlmTask::CategorySuggest && !known_categories.is_empty() {
        user.push_str("\n\nExisting categories to prefer when relevant: ");
        user.push_str(&known_categories.join(", "));
    }

    append_task_options(&mut user, task, options);

    (system, user)
}

fn append_task_options(user: &mut String, task: LlmTask, options: &LlmTaskOptions) {
    if task == LlmTask::Summarize {
        match options.summary_variant {
            SummaryVariant::Plain => {}
            SummaryVariant::Mnemonic => user.push_str(
                "\n\nSummary format: include a compact mnemonic that helps the user remember the note.",
            ),
            SummaryVariant::Acrostic => user.push_str(
                "\n\nSummary format: use a short acrostic where the first letters form a memorable cue.",
            ),
        }
    }

    if matches!(task, LlmTask::Rewrite | LlmTask::GenerateFromSummary) {
        if options.rewrite_mode == RewriteMode::Simplify {
            user.push_str(
                "\n\nRewrite mode: simplify. Remove obvious redundancy and make the note easier to scan without losing meaning.",
            );
        }
        if let Some(card) = &options.style_card {
            user.push_str("\n\nStyle card to follow:");
            user.push_str(&format!(
                "\nName: {}\nDescription: {}\nVerbosity: {}\nPerspective: {}\nReading level: {}\nVoice: {}",
                card.name,
                card.short_description,
                card.verbosity,
                card.perspective,
                card.reading_level,
                card.voice
            ));
            if !card.patterns.is_empty() {
                user.push_str("\nPatterns: ");
                user.push_str(&card.patterns.join("; "));
            }
            if !card.exemplars.is_empty() {
                user.push_str("\nExemplars: ");
                user.push_str(&card.exemplars.join("\n---\n"));
            }
        }
        if let Some(overrides) = &options.style_overrides {
            if !overrides.trim().is_empty() {
                user.push_str("\n\nStyle overrides for this run:\n");
                user.push_str(overrides.trim());
            }
        }
    }
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

    #[test]
    fn rewrite_prompt_includes_style_card_and_overrides() {
        let options = LlmTaskOptions {
            style_card: Some(PromptStyleCard {
                id: "builtin:test".into(),
                name: "Plainspoken".into(),
                short_description: "Direct, practical prose.".into(),
                verbosity: "succinct".into(),
                perspective: "second_person".into(),
                reading_level: "accessible".into(),
                voice: "active".into(),
                patterns: vec!["Short sentences.".into()],
                exemplars: vec!["Do the next clear thing.".into()],
            }),
            style_overrides: Some("Prefer bullet-like paragraphs.".into()),
            rewrite_mode: RewriteMode::Simplify,
            ..Default::default()
        };
        let (_sys, user) = build_with_options(LlmTask::Rewrite, &note(), &[], &options);
        assert!(user.contains("Plainspoken"));
        assert!(user.contains("Short sentences."));
        assert!(user.contains("Prefer bullet-like paragraphs."));
        assert!(user.contains("simplify"));
    }

    #[test]
    fn generate_from_summary_uses_summary_without_body() {
        let (_sys, user) = build_with_options(
            LlmTask::GenerateFromSummary,
            &note(),
            &[],
            &Default::default(),
        );
        assert!(user.contains("Turn off power first"));
        assert!(!user.contains("switch off the breaker"));
    }
}
