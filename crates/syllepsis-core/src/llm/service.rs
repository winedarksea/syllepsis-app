//! [`LlmService`]: routes a task to its model, builds the prompt, calls the provider, and wraps
//! the reply as a [`Proposal`]. This is the one object the app layer drives for LLM features; it
//! owns the provider seam and the routing config so callers never touch either directly.

use serde::Deserialize;

use crate::config::{LlmRouting, ModelRef};
use crate::error::{CoreError, CoreResult};
use crate::llm::prompts::{self, LlmTaskOptions};
use crate::llm::proposal::Proposal;
use crate::llm::provider::{LlmProvider, LlmRequest};
use crate::llm::task::LlmTask;
use crate::model::{FactCheckAssessment, Note};

pub struct LlmService {
    provider: Box<dyn LlmProvider>,
    routing: LlmRouting,
}

impl LlmService {
    pub fn new(provider: Box<dyn LlmProvider>, routing: LlmRouting) -> LlmService {
        LlmService { provider, routing }
    }

    /// Name of the active provider (e.g. `local`).
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Whether the active provider performs real inference.
    pub fn is_live(&self) -> bool {
        self.provider.is_live()
    }

    /// Run `task` over `note` and return the resulting proposal. `known_categories` is only used
    /// by [`LlmTask::CategorySuggest`].
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
        self.generate_with_model_ref_and_options(
            task,
            model_ref,
            note,
            known_categories,
            &LlmTaskOptions::default(),
        )
    }

    pub fn generate_with_options(
        &self,
        task: LlmTask,
        note: &Note,
        known_categories: &[String],
        options: &LlmTaskOptions,
    ) -> CoreResult<Proposal> {
        self.generate_with_model_ref_and_options(
            task,
            task.model_ref(&self.routing).clone(),
            note,
            known_categories,
            options,
        )
    }

    pub fn generate_with_model_ref_and_options(
        &self,
        task: LlmTask,
        model_ref: ModelRef,
        note: &Note,
        known_categories: &[String],
        options: &LlmTaskOptions,
    ) -> CoreResult<Proposal> {
        if !self.provider.is_live() {
            return Err(CoreError::Llm(format!(
                "provider {} is not a model-backed LLM",
                self.provider.name()
            )));
        }
        let (system, user) = prompts::build_with_options(task, note, known_categories, options);
        if self.provider.name() != model_ref.provider {
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
        tracing::info!(
            task = task.as_str(),
            provider = self.provider.name(),
            model = %model_ref.model,
            live = self.provider.is_live(),
            note = %note.id,
            "llm: generating proposal"
        );
        let started = std::time::Instant::now();
        let response = self.provider.complete(&request)?;
        tracing::info!(
            task = task.as_str(),
            provider = self.provider.name(),
            elapsed_ms = started.elapsed().as_millis(),
            chars = response.text.len(),
            "llm: proposal ready"
        );

        Ok(Proposal::new(
            note.id.clone(),
            task,
            model_ref,
            response.text,
            true,
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

/// Parse a fact-check YAML response into an assessment enum and a notes string.
///
/// Accepts optional leading/trailing markdown code fences. Block scalars with missing
/// indentation (a common model quirk) are fixed before parsing so there is a single
/// `serde_yaml` path. Returns `ResponseFailed` only for truly unrecognisable output.
pub fn parse_fact_check_response(text: &str) -> (FactCheckAssessment, String) {
    #[derive(Deserialize)]
    struct FactCheckYaml {
        assessment: String,
        notes: String,
    }

    let stripped = strip_code_fences(text);
    let normalized = fix_unindented_block_scalars(stripped);
    match serde_yaml::from_str::<FactCheckYaml>(&normalized) {
        Ok(parsed) => {
            let assessment = match parsed.assessment.trim().to_ascii_uppercase().as_str() {
                "STRONG_EVIDENCE" => FactCheckAssessment::StrongEvidence,
                "SOME_QUESTIONABLE_POINTS" => FactCheckAssessment::SomeQuestionablePoints,
                "MANY_QUESTIONABLE_POINTS" => FactCheckAssessment::ManyQuestionablePoints,
                "FULL_FAILURE" => FactCheckAssessment::FullFailure,
                "NO_CHECKABLE_CLAIMS" => FactCheckAssessment::NoCheckableClaims,
                _ => FactCheckAssessment::ResponseFailed,
            };
            (assessment, parsed.notes.trim_end().to_string())
        }
        Err(_) => (FactCheckAssessment::ResponseFailed, text.to_string()),
    }
}

/// Indent content lines that follow a bare block-scalar indicator (`key: >` / `key: |`) so
/// `serde_yaml` can parse them. Models sometimes omit the required indentation; valid YAML
/// passes through unchanged (double-indenting an already-indented block is still valid).
fn fix_unindented_block_scalars(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut in_block = false;

    for line in text.lines() {
        if in_block {
            out.push_str("  ");
            out.push_str(line);
        } else {
            out.push_str(line);
            if let Some(colon) = line.find(':') {
                let key = &line[..colon];
                if key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    let val = line[colon + 1..].trim();
                    if matches!(val, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
                        in_block = true;
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

fn strip_code_fences(text: &str) -> &str {
    let s = text.trim();
    let s = s
        .strip_prefix("```yaml")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelRef;
    use crate::llm::provider::LlmResponse;
    use crate::model::ObjectType;

    fn service() -> LlmService {
        LlmService::new(Box::new(LiveProvider), LlmRouting::default())
    }

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
        assert!(p.live);
        assert!(!p.content.is_empty());
    }

    #[test]
    fn proposal_records_provider_and_model_ref() {
        let svc = service();
        let n = note("Always switch off the breaker first.");
        let proposal = svc.generate(LlmTask::Summarize, &n, &[]).unwrap();
        assert_eq!(proposal.provider, "local");
        assert_eq!(proposal.model, crate::onnx::manifest::BUNDLED_LLM_ID);
        assert!(proposal.live);
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

    #[test]
    fn fact_check_parser_handles_each_variant() {
        for (raw, expected) in [
            ("STRONG_EVIDENCE", FactCheckAssessment::StrongEvidence),
            ("SOME_QUESTIONABLE_POINTS", FactCheckAssessment::SomeQuestionablePoints),
            ("MANY_QUESTIONABLE_POINTS", FactCheckAssessment::ManyQuestionablePoints),
            ("FULL_FAILURE", FactCheckAssessment::FullFailure),
            ("NO_CHECKABLE_CLAIMS", FactCheckAssessment::NoCheckableClaims),
        ] {
            let yaml = format!("assessment: {raw}\nnotes: some notes here");
            let (assessment, notes) = parse_fact_check_response(&yaml);
            assert_eq!(assessment, expected, "failed for {raw}");
            assert_eq!(notes, "some notes here");
        }
    }

    #[test]
    fn fact_check_parser_is_case_insensitive() {
        let yaml = "assessment: strong_evidence\nnotes: looks good";
        let (assessment, notes) = parse_fact_check_response(yaml);
        assert_eq!(assessment, FactCheckAssessment::StrongEvidence);
        assert_eq!(notes, "looks good");
    }

    #[test]
    fn fact_check_parser_unknown_variant_gives_response_failed() {
        let yaml = "assessment: UNKNOWN_THING\nnotes: something";
        let (assessment, _) = parse_fact_check_response(yaml);
        assert_eq!(assessment, FactCheckAssessment::ResponseFailed);
    }

    #[test]
    fn fact_check_parser_invalid_yaml_gives_response_failed_with_raw_text() {
        let raw = "this is not yaml at all :::";
        let (assessment, notes) = parse_fact_check_response(raw);
        assert_eq!(assessment, FactCheckAssessment::ResponseFailed);
        assert_eq!(notes, raw);
    }

    #[test]
    fn fact_check_parser_strips_code_fences() {
        let fenced = "```yaml\nassessment: FULL_FAILURE\nnotes: bad claims\n```";
        let (assessment, notes) = parse_fact_check_response(fenced);
        assert_eq!(assessment, FactCheckAssessment::FullFailure);
        assert_eq!(notes, "bad claims");
    }

    #[test]
    fn fact_check_parser_handles_unindented_block_scalar() {
        // Models sometimes emit `notes: >` with the content on the next line at zero
        // indentation, which is invalid YAML — the line-based fallback must recover it.
        let raw = "assessment: STRONG_EVIDENCE\nnotes: >\nAll claims check out.";
        let (assessment, notes) = parse_fact_check_response(raw);
        assert_eq!(assessment, FactCheckAssessment::StrongEvidence);
        assert_eq!(notes, "All claims check out.");
    }
}
