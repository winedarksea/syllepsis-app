//! [`LlmService`]: routes a task to its model, builds the prompt, calls the provider, and wraps
//! the reply as a [`Proposal`]. This is the one object the app layer drives for LLM features; it
//! owns the provider seam and the routing config so callers never touch either directly.

use crate::config::{LlmRouting, ModelRef};
use crate::error::{CoreError, CoreResult};
use crate::llm::prompts;
use crate::llm::proposal::Proposal;
use crate::llm::provider::{LlmProvider, LlmRequest};
use crate::llm::task::LlmTask;
use crate::model::Note;

pub struct LlmService {
    provider: Box<dyn LlmProvider>,
    routing: LlmRouting,
}

impl LlmService {
    pub fn new(provider: Box<dyn LlmProvider>, routing: LlmRouting) -> LlmService {
        LlmService { provider, routing }
    }

    /// Name of the active provider (e.g. `offline`, `anthropic`).
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Whether the active provider performs real inference.
    pub fn is_live(&self) -> bool {
        self.provider.is_live()
    }

    /// Run `task` over `note` and return the resulting proposal. `known_categories` is only used
    /// by [`LlmTask::CategorySuggest`]. For body-replacing tasks an offline provider yields a
    /// no-op proposal (content = the unchanged body) so it can never degrade a note.
    pub fn generate(
        &self,
        task: LlmTask,
        note: &Note,
        known_categories: &[String],
    ) -> CoreResult<Proposal> {
        self.generate_with_model_ref(
            task,
            task.model_ref(&self.routing).clone(),
            note,
            known_categories,
        )
    }

    /// Run `task` with a caller-selected provider/model instead of the configured route. This is
    /// the per-action override path for "use a stronger model once" without mutating book config.
    pub fn generate_with_model_ref(
        &self,
        task: LlmTask,
        model_ref: ModelRef,
        note: &Note,
        known_categories: &[String],
    ) -> CoreResult<Proposal> {
        let (system, user) = prompts::build(task, note, known_categories);
        if self.provider.is_live() && self.provider.name() != model_ref.provider {
            return Err(CoreError::Llm(format!(
                "task {} targets provider {}, but active provider is {}",
                task.as_str(),
                model_ref.provider,
                self.provider.name()
            )));
        }
        let request = LlmRequest {
            task,
            model_ref: model_ref.clone(),
            system,
            user,
        };
        let response = self.provider.complete(&request)?;

        let content = if task.replaces_body() && !self.provider.is_live() {
            note.body.clone()
        } else {
            response.text
        };

        let proposal_model_ref = if self.provider.is_live() {
            model_ref
        } else {
            crate::config::ModelRef::new(self.provider.name(), self.provider.name())
        };

        Ok(Proposal::new(
            note.id.clone(),
            task,
            proposal_model_ref,
            content,
            self.provider.is_live(),
        ))
    }

    /// Convenience over [`generate`] that parses a [`LlmTask::CategorySuggest`] proposal into a
    /// clean list of category names.
    pub fn suggest_categories(
        &self,
        note: &Note,
        known_categories: &[String],
    ) -> CoreResult<Vec<String>> {
        let proposal = self.generate(LlmTask::CategorySuggest, note, known_categories)?;
        Ok(parse_category_list(&proposal.content))
    }
}

/// Parse a model's comma/newline-separated category reply into normalized tag names: lowercase,
/// `#` and surrounding punctuation stripped, spaces collapsed to hyphens, deduplicated.
pub fn parse_category_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split([',', '\n', ';']) {
        let cleaned: String = raw
            .trim()
            .trim_start_matches('#')
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        let token: String = cleaned
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !token.is_empty() && !out.contains(&token) {
            out.push(token);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelRef;
    use crate::llm::offline::OfflineLlmProvider;
    use crate::llm::provider::LlmResponse;
    use crate::model::ObjectType;

    fn service() -> LlmService {
        LlmService::new(Box::new(OfflineLlmProvider::new()), LlmRouting::default())
    }

    fn note(body: &str) -> Note {
        let mut n = Note::new(ObjectType::Note, "Breaker safety", "syllepsis_001");
        n.body = body.into();
        n
    }

    #[test]
    fn generate_summarize_produces_pending_proposal() {
        let svc = service();
        let n = note("Always switch off the breaker first. Then test the wire.");
        let p = svc.generate(LlmTask::Summarize, &n, &[]).unwrap();
        assert_eq!(p.task, LlmTask::Summarize);
        assert_eq!(p.target, n.id);
        assert!(!p.live);
        assert!(!p.content.is_empty());
    }

    #[test]
    fn offline_rewrite_is_a_noop_on_the_body() {
        let svc = service();
        let n = note("the original body text stays exactly the same");
        let p = svc.generate(LlmTask::Rewrite, &n, &[]).unwrap();
        assert_eq!(p.content, n.body, "offline must not alter the body");
    }

    #[test]
    fn suggest_categories_parses_into_clean_tags() {
        let svc = service();
        let n = note("breaker breaker electrical panel kitchen wiring breaker");
        let cats = svc.suggest_categories(&n, &[]).unwrap();
        assert!(cats.contains(&"breaker".to_string()));
        assert!(cats.iter().all(|c| !c.contains(' ') && !c.starts_with('#')));
    }

    #[test]
    fn proposal_records_provider_and_model_ref() {
        let svc = service();
        let n = note("Always switch off the breaker first.");
        let proposal = svc.generate(LlmTask::Summarize, &n, &[]).unwrap();
        assert_eq!(proposal.provider, "offline");
        assert_eq!(proposal.model, "offline");
        assert!(!proposal.live);
    }

    #[test]
    fn live_provider_rejects_mismatched_route_provider() {
        struct LiveProvider;
        impl LlmProvider for LiveProvider {
            fn name(&self) -> &str {
                "local"
            }

            fn complete(&self, _request: &LlmRequest) -> CoreResult<LlmResponse> {
                Ok(LlmResponse { text: "ok".into() })
            }
        }

        let routing = LlmRouting {
            summarize: ModelRef::new("anthropic", "claude-opus"),
            ..Default::default()
        };
        let svc = LlmService::new(Box::new(LiveProvider), routing);
        let err = svc
            .generate(LlmTask::Summarize, &note("body"), &[])
            .unwrap_err();
        assert!(err.to_string().contains("targets provider anthropic"));
    }

    #[test]
    fn live_provider_allows_matching_per_action_override() {
        struct LiveProvider;
        impl LlmProvider for LiveProvider {
            fn name(&self) -> &str {
                "local"
            }

            fn complete(&self, request: &LlmRequest) -> CoreResult<LlmResponse> {
                Ok(LlmResponse {
                    text: format!("{}:ok", request.model_ref.model),
                })
            }
        }

        let svc = LlmService::new(Box::new(LiveProvider), LlmRouting::default());
        let proposal = svc
            .generate_with_model_ref(
                LlmTask::Summarize,
                ModelRef::new("local", "larger-local-model"),
                &note("body"),
                &[],
            )
            .unwrap();

        assert_eq!(proposal.provider, "local");
        assert_eq!(proposal.model, "larger-local-model");
        assert_eq!(proposal.content, "larger-local-model:ok");
        assert!(proposal.live);
    }

    #[test]
    fn category_parser_normalizes_messy_model_output() {
        let parsed = parse_category_list("#Electrical, Main Panel ; safety\nsafety");
        assert_eq!(parsed, vec!["electrical", "main-panel", "safety"]);
    }
}
