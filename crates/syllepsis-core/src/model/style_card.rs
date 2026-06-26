//! Style cards (llm-ai-features.md): capture the writing style of a corpus so an LLM can
//! generate or rewrite text in that style, and so a note's style can be graded against it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Succinct,
    Standard,
    Expansive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Perspective {
    FirstPersonSingular,
    FirstPersonPlural,
    FirstPersonSoliloquy,
    SecondPerson,
    ThirdPersonObjective,
    ThirdPersonOmniscient,
    ThirdPersonLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingLevel {
    Elementary,
    Accessible,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Voice {
    Active,
    Passive,
}

/// A short, representative snippet that exemplifies the style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exemplar {
    pub text: String,
}

/// A description of a recurring pattern or anti-pattern in the style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleCard {
    /// Schema version (for future migrations).
    #[serde(default = "default_card_version")]
    pub version: u32,
    pub name: String,
    pub short_description: String,
    pub verbosity: Verbosity,
    pub perspective: Perspective,
    pub reading_level: ReadingLevel,
    pub voice: Voice,
    #[serde(default)]
    pub patterns: Vec<Pattern>,
    #[serde(default)]
    pub exemplars: Vec<Exemplar>,
    /// Openly accessible source texts for the style.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_urls: Vec<String>,
    /// Style vector per embedding model, keyed by model id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub style_vectors: BTreeMap<String, Vec<f32>>,
}

fn default_card_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let card = StyleCard {
            version: 1,
            name: "Terse Stoic".into(),
            short_description: "Terse, aphoristic life advice.".into(),
            verbosity: Verbosity::Succinct,
            perspective: Perspective::SecondPerson,
            reading_level: ReadingLevel::Accessible,
            voice: Voice::Active,
            patterns: vec![Pattern {
                text: "Short declarative sentences.".into(),
            }],
            exemplars: vec![Exemplar {
                text: "We suffer more in imagination than in reality.".into(),
            }],
            source_urls: vec![],
            style_vectors: BTreeMap::new(),
        };
        let yaml = serde_yaml::to_string(&card).unwrap();
        let back: StyleCard = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(card, back);
    }
}
