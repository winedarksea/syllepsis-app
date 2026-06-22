//! The descriptive "what kind of statement is this" schema from object-types.md. These are
//! orthogonal to [`super::ObjectType`]: a `note` can be a `hypothesis` grounded in
//! `science_and_data`, etc. All fields have doc-specified defaults so a bare note needs no
//! frontmatter.

use serde::{Deserialize, Serialize};

/// What sort of claim the text makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatementType {
    Hypothesis,
    FactualClaim,
    RuleOrRequirement,
    Principle,
    Preference,
    Procedure,
    Context,
    AnalysisOrInterpretation,
    Narrative,
    #[default]
    Idea,
}

/// What the statement rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    ScienceAndData,
    RegulationOrStandard,
    LogicAndReasoning,
    TraditionAndCulture,
    EstablishedLoreOrFiction,
    LivedExperience,
    PersonalPreference,
    #[default]
    None,
}

/// How verifiable the statement is — drives whether a fact-check makes sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Checkability {
    ObjectivelyCheckable,
    PartlyJudgmentBased,
    SubjectiveOrPersonal,
    #[default]
    None,
}

/// How settled the content is expected to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Settled,
    #[default]
    Evolving,
    Tentative,
}

/// Editorial importance (distinct from todo priority `p:0..3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    #[default]
    Standard,
    Important,
    Core,
}

/// The aggregate descriptive classification stored in frontmatter. Every field defaults, so
/// `Classification::default()` is the no-frontmatter baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Classification {
    pub statement_type: StatementType,
    pub basis: Basis,
    pub checkability: Checkability,
    pub stability: Stability,
    pub priority: Priority,
    pub starred: bool,
    /// Free-form rhetorical tags, e.g. `["anecdote", "metaphor"]`.
    pub stylistic_elements: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_doc() {
        let c = Classification::default();
        assert_eq!(c.statement_type, StatementType::Idea);
        assert_eq!(c.basis, Basis::None);
        assert_eq!(c.stability, Stability::Evolving);
        assert_eq!(c.priority, Priority::Standard);
        assert!(!c.starred);
        assert!(c.stylistic_elements.is_empty());
    }

    #[test]
    fn serializes_snake_case() {
        let yaml = serde_yaml::to_string(&StatementType::FactualClaim).unwrap();
        assert_eq!(yaml.trim(), "factual_claim");
    }
}
