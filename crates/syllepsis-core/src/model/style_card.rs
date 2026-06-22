//! Style cards (llm-ai-features.md): capture the writing style of a corpus so an LLM can
//! generate or rewrite text in that style, and so a note's style can be graded against it.
//!
//! The struct ships now; the creation workflow (corpus → embeddings → exemplar discovery →
//! LLM draft → human finalize) and prompt-and-rerank land in the LLM phase. Cards are
//! **versioned** to support future attribute changes, and may store one style vector per
//! embedding model (keyed by model id) so multiple embedders can coexist.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Technical,
    Instructional,
    Persuasive,
    Narrative,
    Reflective,
    Administrative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tenor {
    Intimate,
    Peer,
    ExpertToPeer,
    ExpertToNovice,
    Institutional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Spoken,
    ConversationalWritten,
    EditedWritten,
    FormalWritten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Sparse,
    Moderate,
    Dense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Texture {
    Plain,
    Polished,
    Vivid,
    Aphoristic,
    Procedural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Organization {
    ConclusionFirst,
    Stepwise,
    Narrative,
    CompareContrast,
    ProblemSolution,
}

/// A short, representative snippet that exemplifies the style, with a note on what it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exemplar {
    pub text: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleCard {
    /// Schema version of the card itself (for future attribute migrations).
    #[serde(default = "default_card_version")]
    pub version: u32,
    pub short_description: String,
    pub field: Field,
    pub tenor: Tenor,
    pub mode: Mode,
    pub density: Density,
    pub texture: Texture,
    pub organization: Organization,
    #[serde(default)]
    pub exemplars: Vec<Exemplar>,
    /// Openly accessible source texts for the style (e.g. Shakespeare sonnets URLs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_urls: Vec<String>,
    /// Style vector per embedding model, keyed by model id — so the vector is always
    /// interpreted against the model that produced it. Populated during creation.
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
            short_description: "Terse, aphoristic life advice.".into(),
            field: Field::Reflective,
            tenor: Tenor::ExpertToNovice,
            mode: Mode::EditedWritten,
            density: Density::Dense,
            texture: Texture::Aphoristic,
            organization: Organization::ConclusionFirst,
            exemplars: vec![Exemplar {
                text: "We suffer more in imagination than in reality.".into(),
                note: "Compression + concrete contrast.".into(),
            }],
            source_urls: vec![],
            style_vectors: BTreeMap::new(),
        };
        let yaml = serde_yaml::to_string(&card).unwrap();
        let back: StyleCard = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(card, back);
    }
}
