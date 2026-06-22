//! Splitting a note's text into overlapping windows before embedding.
//!
//! A long note embedded as a single vector blurs distinct ideas into one averaged point;
//! chunking keeps each passage's signal sharp so vector search can match a query against the
//! *part* of a note that is relevant. The overlap carries a little context across the seam so
//! a sentence split between two chunks is still findable from either side.
//!
//! "Tokens" here are approximated by whitespace-separated words. A real model's tokenizer
//! counts sub-word pieces; the [`EmbeddingProvider`](super::EmbeddingProvider) that owns the
//! true tokenizer can override [`chunk_text`] if it needs exact boundaries. Word counting is a
//! deliberate, dependency-free approximation that keeps the boundary logic testable.

use crate::config::EmbeddingConfig;

/// One window of a note's text together with its position in the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Zero-based index of this chunk within the note (stable id for a per-chunk vector).
    pub index: usize,
    /// The chunk's text.
    pub text: String,
}

/// Break `text` into overlapping chunks of at most `limit` words, each starting `limit -
/// overlap` words after the previous. Short text yields a single chunk; empty/whitespace text
/// yields none. `overlap` is clamped below `limit` so the window always advances.
pub fn chunk_text(text: &str, cfg: &EmbeddingConfig) -> Vec<Chunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let limit = cfg.chunk_token_limit.max(1);
    if words.len() <= limit {
        return vec![Chunk {
            index: 0,
            text: words.join(" "),
        }];
    }
    // Guarantee forward progress even if a config sets overlap >= limit.
    let stride = limit.saturating_sub(cfg.chunk_overlap_tokens).max(1);

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + limit).min(words.len());
        chunks.push(Chunk {
            index: chunks.len(),
            text: words[start..end].join(" "),
        });
        if end == words.len() {
            break;
        }
        start += stride;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(limit: usize, overlap: usize) -> EmbeddingConfig {
        EmbeddingConfig {
            chunk_token_limit: limit,
            chunk_overlap_tokens: overlap,
            dimensions: 8,
            ..Default::default()
        }
    }

    #[test]
    fn short_text_is_one_chunk() {
        let chunks = chunk_text("just a few words", &cfg(10, 2));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "just a few words");
    }

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk_text("   \n  ", &cfg(10, 2)).is_empty());
    }

    #[test]
    fn long_text_chunks_with_overlap_and_stable_indices() {
        let text: String = (0..10)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_text(&text, &cfg(4, 1)); // stride 3
                                                    // windows: [0..4] [3..7] [6..10]
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].text, "w0 w1 w2 w3");
        assert_eq!(chunks[1].text, "w3 w4 w5 w6");
        assert_eq!(chunks[2].text, "w6 w7 w8 w9");
        assert_eq!(chunks[2].index, 2);
    }

    #[test]
    fn pathological_overlap_still_terminates() {
        let text: String = (0..20).map(|i| format!("w{i} ")).collect();
        // overlap >= limit would stall; stride is clamped to 1.
        let chunks = chunk_text(&text, &cfg(3, 9));
        assert!(chunks.len() <= 20);
        assert_eq!(chunks.last().unwrap().text, "w17 w18 w19");
    }
}
