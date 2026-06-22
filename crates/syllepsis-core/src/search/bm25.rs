//! Okapi BM25 — the keyword-ranking half of search.
//!
//! BM25 scores a document for a query by summing, per query term: how often the term appears
//! in the document (with diminishing returns, tuned by `k1`), weighted by how rare the term is
//! across the whole book (idf), and discounted if the document is much longer than average
//! (length normalization, tuned by `b`). It is the standard lexical relevance ranking and what
//! "FTS5/BM25" in the design docs refers to; we compute it directly so search has no native
//! SQLite dependency, while staying drop-in compatible with a future sqlite-vec/FTS5 backend.

use std::collections::HashMap;

use crate::config::SearchConfig;
use crate::text::tokenize;

/// An inverted index over a fixed set of documents, ready to score BM25 queries.
pub struct Bm25Index {
    /// Per-document term frequencies (token → count), parallel to `doc_ids`.
    term_freqs: Vec<HashMap<String, u32>>,
    /// Token length of each document.
    doc_lengths: Vec<usize>,
    /// Number of documents containing each token (for idf).
    doc_freq: HashMap<String, u32>,
    /// Mean document length, the BM25 length-normalization baseline.
    avg_doc_length: f32,
    doc_count: usize,
}

impl Bm25Index {
    /// Build the index from each document's full text. Document `i` is addressed by its index,
    /// which the caller maps back to a note id.
    pub fn build(documents: &[String]) -> Bm25Index {
        let mut term_freqs = Vec::with_capacity(documents.len());
        let mut doc_lengths = Vec::with_capacity(documents.len());
        let mut doc_freq: HashMap<String, u32> = HashMap::new();
        let mut total_length = 0usize;

        for doc in documents {
            let tokens = tokenize(doc);
            total_length += tokens.len();
            let mut freqs: HashMap<String, u32> = HashMap::new();
            for token in tokens {
                *freqs.entry(token).or_insert(0) += 1;
            }
            for term in freqs.keys() {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
            doc_lengths.push(freqs.values().map(|c| *c as usize).sum());
            term_freqs.push(freqs);
        }

        let doc_count = documents.len();
        let avg_doc_length = if doc_count > 0 {
            total_length as f32 / doc_count as f32
        } else {
            0.0
        };

        Bm25Index {
            term_freqs,
            doc_lengths,
            doc_freq,
            avg_doc_length,
            doc_count,
        }
    }

    /// Score every document against `query`, returning `(doc_index, score)` for documents that
    /// match at least one query term, sorted by descending score. Documents with no overlap are
    /// omitted (score 0).
    pub fn score(&self, query: &str, cfg: &SearchConfig) -> Vec<(usize, f32)> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() || self.doc_count == 0 {
            return Vec::new();
        }

        let mut scores = vec![0.0f32; self.doc_count];
        for term in &query_terms {
            let Some(&df) = self.doc_freq.get(term) else {
                continue;
            };
            let idf = self.idf(df);
            for (doc, freqs) in self.term_freqs.iter().enumerate() {
                if let Some(&tf) = freqs.get(term) {
                    scores[doc] += idf * self.tf_component(tf, self.doc_lengths[doc], cfg);
                }
            }
        }

        let mut ranked: Vec<(usize, f32)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, s)| *s > 0.0)
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
    }

    /// Probabilistic idf with the standard `+1` so it is never negative even for a term in
    /// every document.
    fn idf(&self, doc_freq: u32) -> f32 {
        let n = self.doc_count as f32;
        let df = doc_freq as f32;
        (((n - df + 0.5) / (df + 0.5)) + 1.0).ln()
    }

    /// The term-frequency saturation + length-normalization factor of BM25.
    fn tf_component(&self, tf: u32, doc_length: usize, cfg: &SearchConfig) -> f32 {
        let tf = tf as f32;
        let length_ratio = if self.avg_doc_length > 0.0 {
            doc_length as f32 / self.avg_doc_length
        } else {
            1.0
        };
        let denom = tf + cfg.bm25_k1 * (1.0 - cfg.bm25_b + cfg.bm25_b * length_ratio);
        (tf * (cfg.bm25_k1 + 1.0)) / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Vec<String> {
        vec![
            "the kitchen has electrical wiring and a breaker panel".into(),
            "the garden needs roses soil and compost for watering".into(),
            "electrical safety means turning off the breaker first".into(),
        ]
    }

    #[test]
    fn ranks_the_most_relevant_document_first() {
        let index = Bm25Index::build(&docs());
        let ranked = index.score("breaker panel", &SearchConfig::default());
        assert_eq!(ranked[0].0, 0, "doc 0 mentions both breaker and panel");
    }

    #[test]
    fn non_matching_documents_are_excluded() {
        let index = Bm25Index::build(&docs());
        let ranked = index.score("roses compost", &SearchConfig::default());
        // Only the garden doc matches.
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].0, 1);
    }

    #[test]
    fn empty_query_or_corpus_scores_nothing() {
        let index = Bm25Index::build(&docs());
        assert!(index.score("   ", &SearchConfig::default()).is_empty());
        let empty = Bm25Index::build(&[]);
        assert!(empty.score("anything", &SearchConfig::default()).is_empty());
    }

    #[test]
    fn rarer_terms_outweigh_common_ones() {
        let index = Bm25Index::build(&docs());
        // "the" appears everywhere (low idf); "compost" is unique (high idf). A doc with
        // compost should rank above one matching only "the".
        let ranked = index.score("the compost", &SearchConfig::default());
        assert_eq!(ranked[0].0, 1);
    }
}
