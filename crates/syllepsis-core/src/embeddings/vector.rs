//! The [`Embedding`] vector type and the pure linear-algebra it needs.
//!
//! This is deliberately provider-agnostic: whatever produces the numbers (the default
//! feature-hashing embedder, or an ONNX model later) hands back an `Embedding`, and the
//! search layer only ever talks to it through cosine similarity and averaging. Keeping the
//! math here — with no model, no I/O — is what lets the provider behind it be swapped freely.

use serde::{Deserialize, Serialize};

/// A dense vector in embedding space. Stored as `f32` to halve the memory of a large book's
/// worth of vectors versus `f64`, which is more than enough precision for cosine ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    /// Wrap a raw vector.
    pub fn new(values: Vec<f32>) -> Embedding {
        Embedding(values)
    }

    /// A zero vector of the given width (the neutral element for averaging).
    pub fn zeros(dimensions: usize) -> Embedding {
        Embedding(vec![0.0; dimensions])
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Euclidean (L2) norm.
    pub fn magnitude(&self) -> f32 {
        self.0.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    /// Scale to unit length in place. A zero vector is left untouched (it has no direction).
    pub fn normalize(&mut self) {
        let mag = self.magnitude();
        if mag > f32::EPSILON {
            for v in &mut self.0 {
                *v /= mag;
            }
        }
    }

    /// Cosine similarity in `[-1, 1]` (1 = identical direction). Mismatched lengths or a
    /// zero-magnitude operand yield `0.0` — "no signal" rather than a panic or NaN, so a blank
    /// note never poisons a ranking.
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let denom = self.magnitude() * other.magnitude();
        if denom > f32::EPSILON {
            dot / denom
        } else {
            0.0
        }
    }

    /// The component-wise mean of several embeddings, re-normalized to unit length. This is the
    /// operation behind a category vector (the average of its members) and a note's overall
    /// vector (the average of its chunks). Returns `None` if `items` is empty or ragged.
    pub fn average<'a>(items: impl IntoIterator<Item = &'a Embedding>) -> Option<Embedding> {
        let mut iter = items.into_iter();
        let first = iter.next()?;
        let mut sum = first.0.clone();
        let mut count = 1usize;
        for item in iter {
            if item.0.len() != sum.len() {
                return None;
            }
            for (acc, v) in sum.iter_mut().zip(&item.0) {
                *acc += v;
            }
            count += 1;
        }
        for v in &mut sum {
            *v /= count as f32;
        }
        let mut avg = Embedding(sum);
        avg.normalize();
        Some(avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_are_maximally_similar() {
        let a = Embedding::new(vec![0.3, 0.4, 0.5]);
        assert!((a.cosine_similarity(&a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn orthogonal_vectors_have_zero_similarity() {
        let a = Embedding::new(vec![1.0, 0.0]);
        let b = Embedding::new(vec![0.0, 1.0]);
        assert!(a.cosine_similarity(&b).abs() < 1e-6);
    }

    #[test]
    fn similarity_ignores_magnitude() {
        let a = Embedding::new(vec![1.0, 1.0]);
        let scaled = Embedding::new(vec![5.0, 5.0]);
        assert!((a.cosine_similarity(&scaled) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mismatched_or_zero_vectors_are_safe() {
        let a = Embedding::new(vec![1.0, 2.0, 3.0]);
        let short = Embedding::new(vec![1.0, 2.0]);
        let zero = Embedding::zeros(3);
        assert_eq!(a.cosine_similarity(&short), 0.0);
        assert_eq!(a.cosine_similarity(&zero), 0.0);
    }

    #[test]
    fn normalize_yields_unit_length() {
        let mut v = Embedding::new(vec![3.0, 4.0]);
        v.normalize();
        assert!((v.magnitude() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn average_is_the_normalized_centroid() {
        let a = Embedding::new(vec![1.0, 0.0]);
        let b = Embedding::new(vec![0.0, 1.0]);
        let avg = Embedding::average([&a, &b]).unwrap();
        // Centroid (0.5, 0.5) normalized → both components equal and unit length.
        assert!((avg.0[0] - avg.0[1]).abs() < 1e-6);
        assert!((avg.magnitude() - 1.0).abs() < 1e-6);
        assert!(Embedding::average(Vec::<&Embedding>::new()).is_none());
    }
}
