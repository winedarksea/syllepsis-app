//! The shapes search returns. These cross the app boundary directly (like
//! [`crate::sort::RenderItem`]), so they are `Serialize`/`Deserialize` and hold owned,
//! UI-ready strings rather than note handles.

use serde::{Deserialize, Serialize};

/// One result row: enough to render a card and open the note, plus its fused relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub note_id: String,
    pub title: String,
    pub summary: String,
    /// A short body excerpt around the match (or the start of the note).
    pub snippet: String,
    pub categories: Vec<String>,
    /// Fused RRF score; only comparable within one result set.
    pub score: f32,
}

/// How many results fall under a category — the facet sidebar of [`SearchResults`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetCount {
    pub category: String,
    pub count: usize,
}

/// A full search response: ranked hits plus the category facet breakdown of the *unfiltered*
/// matches, so the UI can show "Electrical (12)" even while a filter is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub facets: Vec<FacetCount>,
}

/// A neighbor in the related carousel: a note near the focused one in embedding space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatedNote {
    pub note_id: String,
    pub title: String,
    pub summary: String,
    pub categories: Vec<String>,
    /// Cosine similarity to the focused note, after any category upweight.
    pub similarity: f32,
    /// Whether this neighbor shares a category with the focused note (drove the upweight).
    pub shares_category: bool,
}

/// Two notes embedded so closely they may be duplicates worth merging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub a_id: String,
    pub a_title: String,
    pub b_id: String,
    pub b_title: String,
    pub similarity: f32,
}

/// A note so dissimilar from everything else that it may be an orphan idea ("blind spot").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlindSpot {
    pub note_id: String,
    pub title: String,
    /// Similarity to its single nearest neighbor — low means weakly connected.
    pub nearest_similarity: f32,
}

/// The embedding-health report surfaced in the diagnostics view (llm-ai-features.md: duplication
/// and reverse-similarity / blind-spot detection).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingDiagnostics {
    pub duplicates: Vec<DuplicatePair>,
    pub blind_spots: Vec<BlindSpot>,
}
