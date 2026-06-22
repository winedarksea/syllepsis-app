//! The default, dependency-free [`EmbeddingProvider`]: feature hashing (the "hashing trick").
//!
//! Each token is hashed to a bucket in `[0, dimensions)` and its count accumulated there, with
//! a second hash deciding the sign so unrelated tokens that collide tend to cancel rather than
//! reinforce. The vector is then unit-normalized. The result is a deterministic bag-of-words
//! embedding: texts sharing many words score high cosine similarity, disjoint texts score near
//! zero. It captures lexical overlap, not deep semantics — but it needs no model download, runs
//! instantly, and gives the search and diagnostics layers something real and reproducible to
//! rank against until the ONNX model lands behind the same [`EmbeddingProvider`] trait.

use std::hash::{Hash, Hasher};

use crate::embeddings::provider::{EmbeddingProvider, ProviderInfo};
use crate::embeddings::vector::Embedding;
use crate::text::tokenize;

/// Feature-hashing embedder at a fixed dimensionality.
#[derive(Debug, Clone)]
pub struct HashingEmbedder {
    dimensions: usize,
}

impl HashingEmbedder {
    /// Build an embedder emitting `dimensions`-wide vectors (clamped to at least 1).
    pub fn new(dimensions: usize) -> HashingEmbedder {
        HashingEmbedder {
            dimensions: dimensions.max(1),
        }
    }
}

/// Stable hash of a token. `DefaultHasher` (SipHash) is deterministic across runs of the same
/// build, which is all the provider contract requires.
fn token_hash(token: &str, salt: u8) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut hasher);
    token.hash(&mut hasher);
    hasher.finish()
}

impl EmbeddingProvider for HashingEmbedder {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn embed(&self, text: &str) -> Embedding {
        let mut values = vec![0.0f32; self.dimensions];
        for token in tokenize(text) {
            let bucket = (token_hash(&token, 0) as usize) % self.dimensions;
            // Independent sign hash so colliding-but-unrelated tokens cancel on average.
            let sign = if token_hash(&token, 1) & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            values[bucket] += sign;
        }
        let mut embedding = Embedding::new(values);
        embedding.normalize();
        embedding
    }
}

impl ProviderInfo for HashingEmbedder {
    fn name(&self) -> &str {
        "hashing-bow"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic() {
        let e = HashingEmbedder::new(64);
        assert_eq!(e.embed("the kitchen wiring"), e.embed("the kitchen wiring"));
    }

    #[test]
    fn emits_requested_dimensionality_and_unit_length() {
        let e = HashingEmbedder::new(128);
        let v = e.embed("some words here");
        assert_eq!(v.len(), 128);
        assert!((v.magnitude() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn shared_vocabulary_scores_higher_than_disjoint() {
        let e = HashingEmbedder::new(512);
        let base = e.embed("kitchen electrical panel wiring breaker");
        let near = e.embed("kitchen electrical panel layout");
        let far = e.embed("philosophy ethics virtue happiness");
        assert!(
            base.cosine_similarity(&near) > base.cosine_similarity(&far),
            "overlapping vocabulary should be nearer than unrelated text"
        );
    }

    #[test]
    fn blank_text_is_a_zero_vector() {
        let e = HashingEmbedder::new(32);
        assert!(e.embed("   %%only a comment%%  ").magnitude() < 1e-6);
    }
}
