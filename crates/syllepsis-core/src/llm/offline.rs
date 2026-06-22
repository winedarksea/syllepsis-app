//! The built-in, no-network [`LlmProvider`]: deterministic heuristics so every LLM flow works
//! without a configured model.
//!
//! It is honest about its limits — it cannot truly fact-check or argue — but it produces
//! deterministic, input-derived text so the generate → proposal → accept/reject pipeline is
//! exercised end-to-end and unit-testable. For body-replacing tasks ([`LlmTask::Grammar`],
//! [`LlmTask::Rewrite`]) the [`service`](super::service) treats offline output as a no-op, so
//! nothing here can corrupt a note. A real model added behind [`LlmProvider`] supersedes it.

use crate::error::CoreResult;
use crate::llm::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::llm::task::LlmTask;
use crate::text::tokenize;

/// Words too common to be useful category suggestions.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "for", "with", "is", "are",
    "was", "were", "be", "this", "that", "it", "as", "at", "by", "from", "title", "summary",
];

/// Number of keyword suggestions the offline category heuristic emits.
const OFFLINE_CATEGORY_COUNT: usize = 3;

#[derive(Debug, Clone, Default)]
pub struct OfflineLlmProvider;

impl OfflineLlmProvider {
    pub fn new() -> OfflineLlmProvider {
        OfflineLlmProvider
    }
}

impl LlmProvider for OfflineLlmProvider {
    fn name(&self) -> &str {
        "offline"
    }

    fn is_live(&self) -> bool {
        false
    }

    fn complete(&self, request: &LlmRequest) -> CoreResult<LlmResponse> {
        let text = match request.task {
            LlmTask::Summarize => first_sentence(&request.user),
            LlmTask::FactCheck => {
                format!(
                    "No automated fact-check while offline. Claim to review: {}",
                    first_sentence(&request.user)
                )
            }
            LlmTask::DevilsAdvocate => {
                format!(
                    "Counterpoint to consider against: {}",
                    first_sentence(&request.user)
                )
            }
            // Body-replacing tasks: echo the input; the service treats offline as a no-op.
            LlmTask::Grammar | LlmTask::Rewrite => request.user.clone(),
            LlmTask::CategorySuggest => {
                top_keywords(&request.user, OFFLINE_CATEGORY_COUNT).join(", ")
            }
        };
        Ok(LlmResponse { text })
    }
}

/// The first sentence of `text`, trimmed; falls back to the whole (trimmed) string.
fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    for (i, c) in trimmed.char_indices() {
        if matches!(c, '.' | '!' | '?') {
            return trimmed[..=i].trim().to_string();
        }
    }
    trimmed.to_string()
}

/// The most frequent non-stopword tokens, by descending count then first appearance.
fn top_keywords(text: &str, n: usize) -> Vec<String> {
    use std::collections::HashMap;
    let mut counts: HashMap<String, (u32, usize)> = HashMap::new();
    for (order, token) in tokenize(text).into_iter().enumerate() {
        if token.len() < 3 || STOPWORDS.contains(&token.as_str()) {
            continue;
        }
        let entry = counts.entry(token).or_insert((0, order));
        entry.0 += 1;
    }
    let mut ranked: Vec<(String, (u32, usize))> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.1 .1.cmp(&b.1 .1)));
    ranked.into_iter().take(n).map(|(t, _)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(task: LlmTask, user: &str) -> LlmRequest {
        LlmRequest {
            task,
            model_ref: crate::config::ModelRef::new("offline", "offline"),
            system: String::new(),
            user: user.into(),
        }
    }

    #[test]
    fn is_not_live() {
        assert!(!OfflineLlmProvider::new().is_live());
    }

    #[test]
    fn summarize_returns_first_sentence() {
        let p = OfflineLlmProvider::new();
        let out = p
            .complete(&req(LlmTask::Summarize, "First idea here. Second idea."))
            .unwrap();
        assert_eq!(out.text, "First idea here.");
    }

    #[test]
    fn category_suggest_returns_frequent_keywords() {
        let p = OfflineLlmProvider::new();
        let out = p
            .complete(&req(
                LlmTask::CategorySuggest,
                "breaker breaker electrical the the panel kitchen",
            ))
            .unwrap();
        // "breaker" appears most; stopword "the" excluded.
        assert!(out.text.starts_with("breaker"));
        assert!(!out.text.contains("the"));
    }

    #[test]
    fn is_deterministic() {
        let p = OfflineLlmProvider::new();
        let a = p
            .complete(&req(LlmTask::FactCheck, "claim one. claim two."))
            .unwrap();
        let b = p
            .complete(&req(LlmTask::FactCheck, "claim one. claim two."))
            .unwrap();
        assert_eq!(a, b);
    }
}
