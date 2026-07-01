//! [`SearchEngine`] — the one object the app queries for retrieval, related notes, and
//! embedding diagnostics.
//!
//! It is built once from a snapshot of the book's notes: it indexes them for BM25, keeps their
//! combined text for exact match, and embeds them for vector search. A query runs all three
//! retrievers and fuses their rankings with RRF (see [`super::rrf`]); related-notes and
//! diagnostics reuse the same precomputed note vectors. Building per query is fine for the
//! POC's book sizes and keeps the engine stateless; persisting the index/vectors to
//! `_derived/` SQLite/FTS5 persistence is a drop-in optimization behind this same type.

use std::collections::{BTreeMap, HashMap};

use crate::config::{Config, SearchConfig};
use crate::embeddings::note::{embed_notes, NoteVectors};
use crate::embeddings::provider::EmbeddingProvider;
use crate::markdown::dialect::strip_comments;
use crate::model::Note;
use crate::search::bm25::Bm25Index;
use crate::search::exact::match_exact;
use crate::search::filter::SearchFilter;
use crate::search::results::{
    BlindSpot, DuplicatePair, EmbeddingDiagnostics, EmptyNote, FacetCount, RelatedNote, SearchHit,
    SearchRankingSignals, SearchResults,
};
use crate::search::rrf::reciprocal_rank_fusion;
use crate::text::tokenize;

/// Characters of body shown in a result snippet.
const SNIPPET_CHARS: usize = 200;
/// How far before the first match the snippet window begins.
const SNIPPET_LEAD_CHARS: usize = 40;

pub struct SearchEngine {
    notes: Vec<Note>,
    /// Combined title+summary+body (comments stripped) per note, for exact matching.
    documents: Vec<String>,
    bm25: Bm25Index,
    vectors: Vec<NoteVectors>,
    provider: Box<dyn EmbeddingProvider>,
    search_cfg: SearchConfig,
}

impl SearchEngine {
    /// Index and embed `notes`. The provider embeds both the corpus now and queries later, so
    /// the same vector space is used throughout.
    pub fn build(
        notes: Vec<Note>,
        provider: Box<dyn EmbeddingProvider>,
        config: &Config,
    ) -> SearchEngine {
        let documents: Vec<String> = notes.iter().map(document_text).collect();
        let bm25 = Bm25Index::build(&documents);
        let vectors = embed_notes(provider.as_ref(), &notes, &config.embedding);
        SearchEngine {
            notes,
            documents,
            bm25,
            vectors,
            provider,
            search_cfg: config.search.clone(),
        }
    }

    /// Build from canonical vectors loaded from synced sidecars. No corpus inference occurs.
    pub fn build_from_vectors(
        notes: Vec<Note>,
        vectors: Vec<NoteVectors>,
        config: &Config,
    ) -> SearchEngine {
        debug_assert_eq!(notes.len(), vectors.len());
        let documents: Vec<String> = notes.iter().map(document_text).collect();
        let bm25 = Bm25Index::build(&documents);
        SearchEngine {
            notes,
            documents,
            bm25,
            vectors,
            provider: Box::new(crate::embeddings::HashingEmbedder::new(0)),
            search_cfg: config.search.clone(),
        }
    }

    /// Run exact + BM25 + vector retrieval, fuse with RRF, and return hits plus category facets.
    /// `category_filter` (if non-empty) keeps only hits in one of those categories; the facet
    /// counts always reflect the unfiltered matches so the UI can show every available facet.
    pub fn search(&self, query: &str, category_filter: &[String]) -> SearchResults {
        let query_embedding = self.query_embedding(query);
        self.search_with_query_embedding(query, category_filter, Some(&query_embedding))
    }

    pub fn search_with_query_embedding(
        &self,
        query: &str,
        category_filter: &[String],
        query_embedding: Option<&crate::embeddings::Embedding>,
    ) -> SearchResults {
        let exact = ids_only(match_exact(&self.documents, query));
        let bm25 = ids_only(self.bm25.score(query, &self.search_cfg));
        let vector_scored: Vec<(usize, f32)> = query_embedding
            .map(|embedding| self.vector_ranking_embedding(embedding))
            .unwrap_or_default();
        let vector: Vec<usize> = vector_scored.iter().map(|(i, _)| *i).collect();
        let vector_cosines: HashMap<usize, f32> = vector_scored.into_iter().collect();

        let exact_ranks = rank_map(&exact);
        let bm25_ranks = rank_map(&bm25);
        let vector_ranks = rank_map(&vector);

        let fused = reciprocal_rank_fusion(&[exact, bm25, vector], &self.search_cfg);

        let facets = self.facet_counts(fused.iter().map(|(idx, _)| *idx));

        let hits: Vec<SearchHit> = fused
            .into_iter()
            .filter(|(idx, _)| self.passes_category_slice(*idx, category_filter))
            .take(self.search_cfg.result_limit)
            .map(|(idx, score)| {
                let ranking_signals = ranking_signals(
                    idx,
                    score,
                    &exact_ranks,
                    &bm25_ranks,
                    &vector_ranks,
                    self.search_cfg.rrf_k,
                    &vector_cosines,
                );
                self.hit(idx, score, query, ranking_signals)
            })
            .collect();

        SearchResults { hits, facets }
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn documents(&self) -> &[String] {
        &self.documents
    }

    pub fn vectors(&self) -> &[NoteVectors] {
        &self.vectors
    }

    pub fn search_config(&self) -> &SearchConfig {
        &self.search_cfg
    }

    pub fn query_embedding(&self, query: &str) -> crate::embeddings::Embedding {
        self.provider.embed_query(query)
    }

    pub fn search_hit_for_index(
        &self,
        idx: usize,
        score: f32,
        query: &str,
        ranking_signals: SearchRankingSignals,
    ) -> SearchHit {
        self.hit(idx, score, query, ranking_signals)
    }

    /// Backward-compat category-only filter used by callers that don't have a full SearchFilter.
    pub fn passes_category_filter(&self, idx: usize, filter: &[String]) -> bool {
        self.passes_category_slice(idx, filter)
    }

    /// Full structured filter predicate — used by `SqliteSearchIndex::search`.
    pub fn passes_filter(&self, idx: usize, filter: &SearchFilter) -> bool {
        let note = &self.notes[idx];

        if !filter.categories.is_empty()
            && !note
                .categories
                .iter()
                .any(|c| filter.categories.contains(c))
        {
            return false;
        }
        if let Some(after) = filter.updated_after {
            if note.metadata.dates.updated < after {
                return false;
            }
        }
        let body_len = note.body.chars().count();
        if filter.min_body_len.is_some_and(|min| body_len < min) {
            return false;
        }
        if filter.max_body_len.is_some_and(|max| body_len > max) {
            return false;
        }
        if !filter.object_types.is_empty() && !filter.object_types.contains(&note.object_type) {
            return false;
        }
        if !filter.classifications.is_empty()
            && !filter
                .classifications
                .contains(&note.metadata.classification.kind)
        {
            return false;
        }
        if filter.starred_only && !note.metadata.classification.starred {
            return false;
        }
        true
    }

    pub fn facet_counts_for_indices(
        &self,
        indices: impl Iterator<Item = usize>,
    ) -> Vec<FacetCount> {
        self.facet_counts(indices)
    }

    /// Notes nearest the focused note in embedding space, upweighted when they share a category
    /// (the related carousel). Returns up to `related_limit` neighbors, best first.
    pub fn related(&self, note_id: &str) -> Vec<RelatedNote> {
        let Some(target) = self.index_of(note_id) else {
            return Vec::new();
        };
        let target_vec = &self.vectors[target].centroid;
        if target_vec.magnitude() <= f32::EPSILON {
            return Vec::new();
        }
        let target_cats = &self.notes[target].categories;

        let mut scored: Vec<RelatedNote> = self
            .vectors
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != target)
            .filter_map(|(i, v)| {
                let raw = v.centroid.cosine_similarity(target_vec);
                if raw <= 0.0 {
                    return None;
                }
                let shares = self.notes[i]
                    .categories
                    .iter()
                    .any(|c| target_cats.contains(c));
                let weighted = if shares {
                    (raw * self.search_cfg.category_upweight).min(1.0)
                } else {
                    raw
                };
                Some(RelatedNote {
                    note_id: self.notes[i].id.to_string(),
                    title: self.notes[i].title.clone(),
                    summary: self.notes[i].summary.clone(),
                    categories: self.notes[i].categories.clone(),
                    similarity: weighted,
                    shares_category: shares,
                })
            })
            .collect();

        scored.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
        scored.truncate(self.search_cfg.related_limit);
        scored
    }

    /// Embedding health: near-duplicate pairs and weakly connected "blind spot" notes.
    pub fn diagnostics(&self) -> EmbeddingDiagnostics {
        let mut duplicates = Vec::new();
        for i in 0..self.vectors.len() {
            if self.vectors[i].centroid.magnitude() <= f32::EPSILON {
                continue;
            }
            for j in (i + 1)..self.vectors.len() {
                if self.vectors[j].centroid.magnitude() <= f32::EPSILON {
                    continue;
                }
                let sim = self.vectors[i]
                    .centroid
                    .cosine_similarity(&self.vectors[j].centroid);
                if sim >= self.search_cfg.duplicate_similarity {
                    duplicates.push(DuplicatePair {
                        a_id: self.notes[i].id.to_string(),
                        a_title: self.notes[i].title.clone(),
                        b_id: self.notes[j].id.to_string(),
                        b_title: self.notes[j].title.clone(),
                        similarity: sim,
                    });
                }
            }
        }
        duplicates.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));

        let mut blind_spots = Vec::new();
        for i in 0..self.vectors.len() {
            if self.vectors[i].centroid.magnitude() <= f32::EPSILON {
                continue;
            }
            let nearest = self
                .vectors
                .iter()
                .enumerate()
                .filter(|(j, v)| *j != i && v.centroid.magnitude() > f32::EPSILON)
                .map(|(_, v)| v.centroid.cosine_similarity(&self.vectors[i].centroid))
                .fold(f32::NEG_INFINITY, f32::max);
            // NEG_INFINITY means no other embeddable note exists; not a blind spot, just alone.
            if nearest.is_finite() && nearest < self.search_cfg.blind_spot_similarity {
                blind_spots.push(BlindSpot {
                    note_id: self.notes[i].id.to_string(),
                    title: self.notes[i].title.clone(),
                    nearest_similarity: nearest,
                });
            }
        }
        blind_spots.sort_by(|a, b| a.nearest_similarity.total_cmp(&b.nearest_similarity));

        let mut empty_notes: Vec<EmptyNote> = self
            .vectors
            .iter()
            .enumerate()
            .filter(|(_, v)| v.centroid.magnitude() <= f32::EPSILON)
            .map(|(i, _)| EmptyNote {
                note_id: self.notes[i].id.to_string(),
                title: self.notes[i].title.clone(),
            })
            .collect();
        empty_notes.sort_by(|a, b| a.title.cmp(&b.title));

        EmbeddingDiagnostics {
            duplicates,
            blind_spots,
            empty_notes,
        }
    }

    /// Vector ranking: best passage similarity per note against the embedded query. The query
    /// goes through [`embed_query`](EmbeddingProvider::embed_query) so an asymmetric model applies
    /// its retrieval-instruction prefix; the corpus was embedded with plain `embed`.
    pub(super) fn vector_ranking_embedding(
        &self,
        query_embedding: &crate::embeddings::Embedding,
    ) -> Vec<(usize, f32)> {
        if query_embedding.magnitude() <= f32::EPSILON {
            return Vec::new();
        }
        let mut ranked: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i, v.best_similarity(query_embedding)))
            .filter(|(_, s)| *s > 0.0)
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
    }

    fn index_of(&self, note_id: &str) -> Option<usize> {
        self.notes.iter().position(|n| n.id.to_string() == note_id)
    }

    fn passes_category_slice(&self, idx: usize, filter: &[String]) -> bool {
        filter.is_empty()
            || self.notes[idx]
                .categories
                .iter()
                .any(|c| filter.contains(c))
    }

    fn facet_counts(&self, indices: impl Iterator<Item = usize>) -> Vec<FacetCount> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for idx in indices {
            for cat in &self.notes[idx].categories {
                match counts.get_mut(cat.as_str()) {
                    Some(count) => *count += 1,
                    None => {
                        counts.insert(cat.clone(), 1);
                    }
                }
            }
        }
        let mut facets: Vec<FacetCount> = counts
            .into_iter()
            .map(|(category, count)| FacetCount { category, count })
            .collect();
        // Most populous first; name breaks ties (BTreeMap already gives name order).
        facets.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.category.cmp(&b.category))
        });
        facets
    }

    fn hit(
        &self,
        idx: usize,
        score: f32,
        query: &str,
        ranking_signals: SearchRankingSignals,
    ) -> SearchHit {
        let note = &self.notes[idx];
        SearchHit {
            note_id: note.id.to_string(),
            title: note.title.clone(),
            summary: note.summary.clone(),
            snippet: snippet(&note.body, query),
            categories: note.categories.clone(),
            score,
            ranking_signals,
            object_type: note.object_type,
            classification: note.metadata.classification.kind,
            updated: note.metadata.dates.updated,
            starred: note.metadata.classification.starred,
            body_len: note.body.chars().count(),
            status: note.metadata.status,
            archived: note.metadata.lifecycle.archived,
            marked_for_deletion_at: note.metadata.lifecycle.marked_for_deletion_at,
        }
    }
}

fn rank_map(indices: &[usize]) -> HashMap<usize, usize> {
    indices
        .iter()
        .enumerate()
        .map(|(rank, idx)| (*idx, rank))
        .collect()
}

fn ranking_signals(
    note_index: usize,
    total: f32,
    exact_ranks: &HashMap<usize, usize>,
    bm25_ranks: &HashMap<usize, usize>,
    vector_ranks: &HashMap<usize, usize>,
    rrf_k: f32,
    vector_cosines: &HashMap<usize, f32>,
) -> SearchRankingSignals {
    let exact = exact_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let bm25 = bm25_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let vector = vector_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let vector_similarity = vector_cosines.get(&note_index).copied().unwrap_or(0.0);
    SearchRankingSignals {
        exact,
        bm25,
        vector,
        total,
        vector_similarity,
    }
}

/// The text BM25 and exact match see: title, summary, and body with comments removed.
fn document_text(note: &Note) -> String {
    format!(
        "{}\n{}\n{}",
        note.title,
        note.summary,
        strip_comments(&note.body)
    )
}

/// Drop the scores, keeping just the ranked document indices for RRF.
fn ids_only(ranked: Vec<(usize, f32)>) -> Vec<usize> {
    ranked.into_iter().map(|(idx, _)| idx).collect()
}

/// A short body excerpt, windowed around the first query term when present (else the start).
fn snippet(body: &str, query: &str) -> String {
    let clean = strip_comments(body);
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let lower = trimmed.to_lowercase();
    let center = tokenize(query)
        .iter()
        .filter_map(|t| lower.find(t.as_str()))
        .min()
        .map(|byte| lower[..byte].chars().count())
        .unwrap_or(0);

    let start = center.saturating_sub(SNIPPET_LEAD_CHARS);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let mut excerpt: String = chars[start..end].iter().collect();
    if start > 0 {
        excerpt.insert(0, '…');
    }
    if end < chars.len() {
        excerpt.push('…');
    }
    excerpt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::hashing::HashingEmbedder;
    use crate::model::ObjectType;
    use crate::search::filter::SearchFilter;

    fn note(title: &str, body: &str, cats: &[&str]) -> Note {
        let mut n = Note::new(ObjectType::Note, title, "syllepsis_001");
        n.body = body.to_string();
        n.categories = cats.iter().map(|c| c.to_string()).collect();
        n
    }

    fn engine(notes: Vec<Note>) -> SearchEngine {
        let config = Config::default();
        let provider = Box::new(HashingEmbedder::new(config.embedding.dimensions));
        SearchEngine::build(notes, provider, &config)
    }

    fn corpus() -> Vec<Note> {
        vec![
            note(
                "Kitchen wiring",
                "Install the breaker panel and outlets in the kitchen.",
                &["electrical"],
            ),
            note(
                "Garden beds",
                "Roses need rich soil and regular compost and watering.",
                &["garden"],
            ),
            note(
                "Breaker safety",
                "Always switch the breaker panel off before any electrical work.",
                &["electrical", "safety"],
            ),
            note(
                "Compost guide",
                "A good compost pile balances greens and browns for the garden.",
                &["garden"],
            ),
        ]
    }

    #[test]
    fn finds_relevant_notes_and_ranks_them() {
        let e = engine(corpus());
        let results = e.search("breaker panel", &[]);
        assert!(!results.hits.is_empty());
        // The two electrical notes mention the breaker panel; one of them ranks first.
        assert!(
            results.hits[0].title.to_lowercase().contains("kitchen")
                || results.hits[0].title.to_lowercase().contains("breaker")
        );
    }

    #[test]
    fn category_facets_count_unfiltered_matches() {
        let e = engine(corpus());
        let results = e.search("the", &[]); // a common word appears across notes
        let electrical = results
            .facets
            .iter()
            .find(|f| f.category == "electrical")
            .map(|f| f.count)
            .unwrap_or(0);
        assert!(electrical >= 1);
    }

    #[test]
    fn category_filter_narrows_hits_only() {
        let e = engine(corpus());
        let all = e.search("breaker compost", &[]);
        let filtered = e.search("breaker compost", &["garden".into()]);
        assert!(filtered.hits.len() <= all.hits.len());
        assert!(filtered
            .hits
            .iter()
            .all(|h| h.categories.contains(&"garden".to_string())));
        // Facets still expose the electrical option even though it is filtered out.
        assert!(filtered.facets.iter().any(|f| f.category == "electrical"));
    }

    #[test]
    fn related_prefers_same_category_neighbors() {
        let e = engine(corpus());
        let target = e.notes[1].id.to_string(); // "Garden beds"
        let related = e.related(&target);
        assert!(!related.is_empty());
        // The compost note (same #garden) should be the top related neighbor.
        assert!(related[0].title.to_lowercase().contains("compost"));
        assert!(related[0].shares_category);
    }

    #[test]
    fn diagnostics_flags_near_duplicates() {
        let mut notes = corpus();
        // Add an almost-identical note to the kitchen one.
        notes.push(note(
            "Kitchen wiring copy",
            "Install the breaker panel and outlets in the kitchen.",
            &["electrical"],
        ));
        let e = engine(notes);
        let diag = e.diagnostics();
        assert!(
            diag.duplicates
                .iter()
                .any(|d| d.a_title.contains("Kitchen") && d.b_title.contains("Kitchen")),
            "the duplicated kitchen note should be detected"
        );
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let e = engine(corpus());
        assert!(e.search("   ", &[]).hits.is_empty());
    }

    #[test]
    fn snippet_windows_around_the_match() {
        let body = "intro text ".repeat(10) + "the special breaker keyword is here";
        let s = snippet(&body, "special breaker");
        assert!(s.contains("special breaker"));
        assert!(s.starts_with('…'));
    }

    #[test]
    fn hit_populates_new_fields() {
        let e = engine(corpus());
        let results = e.search("breaker panel", &[]);
        assert!(!results.hits.is_empty());
        let hit = &results.hits[0];
        // object_type should default to Note
        assert_eq!(hit.object_type, ObjectType::Note);
        // body_len reflects char count of the body
        assert!(hit.body_len > 0);
    }

    #[test]
    fn passes_filter_applies_starred_and_type_predicates() {
        let e = engine(corpus());
        // No notes are starred by default — starred_only should exclude all
        let filter = SearchFilter {
            starred_only: true,
            ..Default::default()
        };
        assert!(corpus()
            .iter()
            .enumerate()
            .all(|(idx, _)| !e.passes_filter(idx, &filter)));

        // Empty filter passes everything
        let empty = SearchFilter::default();
        assert!(corpus()
            .iter()
            .enumerate()
            .all(|(idx, _)| e.passes_filter(idx, &empty)));

        // Category filter: only idx 0 and 2 are electrical
        let cat_filter = SearchFilter {
            categories: vec!["electrical".into()],
            ..Default::default()
        };
        assert!(e.passes_filter(0, &cat_filter)); // Kitchen wiring — electrical
        assert!(!e.passes_filter(1, &cat_filter)); // Garden beds — garden only
    }
}
