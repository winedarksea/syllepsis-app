//! The shapes search returns. These cross the app boundary directly (like
//! [`crate::sort::RenderItem`]), so they are `Serialize`/`Deserialize` and hold owned,
//! UI-ready strings rather than note handles.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{NoteStatus, ObjectType};

/// Per-retriever contribution to a hit's final reciprocal-rank-fusion score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchRankingSignals {
    /// Contribution from exact-match ranking.
    pub exact: f32,
    /// Contribution from BM25/FTS ranking.
    pub bm25: f32,
    /// Contribution from vector ranking.
    pub vector: f32,
    /// Sum of the individual contributions (equal to `SearchHit::score`).
    pub total: f32,
    /// Raw cosine similarity of the best matching chunk to the query embedding (0 when no vector
    /// hit — e.g. empty note or no query embedding). Complements the rank-based `vector` signal.
    pub vector_similarity: f32,
}

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
    /// Subtle explainability metadata for the ranking UI.
    pub ranking_signals: SearchRankingSignals,
    /// The note's object type (note, todo, qa, …).
    pub object_type: ObjectType,
    /// When the note was last updated — ISO timestamp for the UI date badge.
    pub updated: DateTime<Utc>,
    /// Whether the note is starred.
    pub starred: bool,
    /// Body length in Unicode scalar values (characters), for the length filter feedback badge.
    pub body_len: usize,
    /// Optional user-managed note status.
    pub status: Option<NoteStatus>,
    /// Lifecycle badges for explicit archived/trash search modes.
    pub archived: bool,
    pub marked_for_deletion_at: Option<DateTime<Utc>>,
}

impl SearchHit {
    /// Relevance score in [0, 1] — mirrors the `searchRelevance` formula in `searchRelevance.ts`.
    pub fn relevance(&self) -> f32 {
        const STRONG_RRF_CHANNEL_CONTRIBUTION: f32 = 1.0 / 61.0;
        const STRONG_LEXICAL_SIGNAL: f32 = STRONG_RRF_CHANNEL_CONTRIBUTION * 2.0;
        const MAX_LEXICAL_RELEVANCE: f32 = 0.72;
        const TWO_CHANNEL_AGREEMENT_BOOST: f32 = 0.04;
        const THREE_CHANNEL_AGREEMENT_BOOST: f32 = 0.08;

        fn clamp01(v: f32) -> f32 {
            if !v.is_finite() {
                return 0.0;
            }
            v.clamp(0.0, 1.0)
        }

        let s = &self.ranking_signals;
        let semantic_relevance = clamp01(s.vector_similarity);
        let lexical_signal = clamp01((s.exact + s.bm25) / STRONG_LEXICAL_SIGNAL);
        let lexical_relevance = lexical_signal * MAX_LEXICAL_RELEVANCE;
        let channel_count = [s.exact, s.bm25, s.vector]
            .iter()
            .filter(|&&v| v > 0.0)
            .count();
        let agreement_boost = match channel_count {
            3.. => THREE_CHANNEL_AGREEMENT_BOOST,
            2 => TWO_CHANNEL_AGREEMENT_BOOST,
            _ => 0.0,
        };
        let primary = semantic_relevance.max(lexical_relevance);
        clamp01(primary + (1.0 - primary) * agreement_boost)
    }
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

/// A note with no body — excluded from vectors and all embedding-based features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyNote {
    pub note_id: String,
    pub title: String,
}

/// The embedding-health report surfaced in the diagnostics view (llm-ai-features.md: duplication
/// and reverse-similarity / blind-spot detection).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingDiagnostics {
    pub duplicates: Vec<DuplicatePair>,
    pub blind_spots: Vec<BlindSpot>,
    pub empty_notes: Vec<EmptyNote>,
}
