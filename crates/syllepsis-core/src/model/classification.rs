//! The descriptive classification schema from object-types.md. Object type is storage shape;
//! this module describes both ordinary statement classes and note subtypes such as Q&A or todo.

use serde::{Deserialize, Serialize};

/// What kind of note or claim this content represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationKind {
    #[default]
    Note,
    Qa,
    Reference,
    Quote,
    Code,
    Todo,
    Idea,
    Hypothesis,
    FactualClaim,
    RuleOrRequirement,
    Principle,
    Preference,
    Procedure,
    Context,
    AnalysisOrInterpretation,
    Narrative,
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
    pub kind: ClassificationKind,
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
        assert_eq!(c.kind, ClassificationKind::Note);
        assert_eq!(c.basis, Basis::None);
        assert_eq!(c.stability, Stability::Evolving);
        assert_eq!(c.priority, Priority::Standard);
        assert!(!c.starred);
        assert!(c.stylistic_elements.is_empty());
    }

    #[test]
    fn serializes_snake_case() {
        let yaml = serde_yaml::to_string(&ClassificationKind::FactualClaim).unwrap();
        assert_eq!(yaml.trim(), "factual_claim");
    }

    #[test]
    fn note_subtypes_serialize_snake_case() {
        let yaml = serde_yaml::to_string(&ClassificationKind::Qa).unwrap();
        assert_eq!(yaml.trim(), "qa");
    }
}
