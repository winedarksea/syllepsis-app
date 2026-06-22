//! Turning a transformer's per-token hidden states into one sentence vector, plus the
//! query-side text shaping an asymmetric embedder needs.
//!
//! These are the pure numerics of the ONNX embedding path, deliberately separated from the
//! [`OnnxEmbedder`](super::onnx) that runs the model so they can be unit-tested without the
//! runtime, a model download, or the `onnx` feature. A model emits a `[seq_len, hidden_size]`
//! matrix of hidden states; how that collapses to a vector depends on the model
//! ([`PoolingStrategy`]). Qwen3-Embedding is causal, so its informative vector is the **last**
//! non-padding token (it alone has attended to the whole sequence); bge-style encoders mean-pool.
//! It is also *asymmetric* — a retrieval query is wrapped in an instruction, a document is not —
//! which is what [`format_query`] handles. Finally, Matryoshka models let the vector be truncated
//! to a shorter prefix for cheaper storage, re-normalized so cosine math is unaffected.

use crate::embeddings::vector::Embedding;
use crate::onnx::manifest::PoolingStrategy;

/// Substitute a retrieval `query` into a manifest's instruction template (the `{query}`
/// placeholder). Used only for the query side of an asymmetric embedder; documents embed raw.
pub fn format_query(instruction_template: &str, query: &str) -> String {
    instruction_template.replace("{query}", query)
}

/// Index of the last attended (non-padding) token, or `None` if every position is padding.
/// Works for both right-padding (the common case) and left-padding by scanning from the end.
pub fn last_token_index(attention_mask: &[i64]) -> Option<usize> {
    attention_mask.iter().rposition(|&m| m != 0)
}

/// Collapse a flat row-major `[seq_len, hidden_size]` hidden-state matrix into one pooled vector
/// per `strategy`, honoring `attention_mask` so padding never dilutes the result. Returns a
/// zero-length vector if the inputs are inconsistent or fully padded, which the caller turns into
/// a zero [`Embedding`] (the existing "no signal, never panic" contract).
pub fn pool(
    strategy: PoolingStrategy,
    hidden_states: &[f32],
    seq_len: usize,
    hidden_size: usize,
    attention_mask: &[i64],
) -> Vec<f32> {
    if hidden_size == 0 || seq_len == 0 || hidden_states.len() != seq_len * hidden_size {
        return Vec::new();
    }
    let row = |t: usize| &hidden_states[t * hidden_size..(t + 1) * hidden_size];

    match strategy {
        PoolingStrategy::Cls => row(0).to_vec(),
        PoolingStrategy::LastToken => match last_token_index(attention_mask) {
            Some(t) if t < seq_len => row(t).to_vec(),
            // No mask given (assume all real) → genuinely the last row.
            _ if attention_mask.is_empty() => row(seq_len - 1).to_vec(),
            _ => Vec::new(),
        },
        PoolingStrategy::Mean => {
            let mut sum = vec![0.0f32; hidden_size];
            let mut counted = 0usize;
            for t in 0..seq_len {
                let attended = attention_mask.get(t).map(|&m| m != 0).unwrap_or(true);
                if !attended {
                    continue;
                }
                for (acc, v) in sum.iter_mut().zip(row(t)) {
                    *acc += v;
                }
                counted += 1;
            }
            if counted == 0 {
                return Vec::new();
            }
            for v in &mut sum {
                *v /= counted as f32;
            }
            sum
        }
    }
}

/// Optionally truncate a pooled vector to its first `target_dims` components (Matryoshka), then
/// unit-normalize. `None`, or a target at/above the native width, keeps the full vector. The
/// re-normalization is what makes a truncated prefix still a valid cosine-space vector.
pub fn matryoshka_embedding(mut values: Vec<f32>, target_dims: Option<usize>) -> Embedding {
    if let Some(dims) = target_dims {
        if dims > 0 && dims < values.len() {
            values.truncate(dims);
        }
    }
    let mut embedding = Embedding::new(values);
    embedding.normalize();
    embedding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_query_instruction_template() {
        let t = "Instruct: retrieve notes\nQuery: {query}";
        assert_eq!(
            format_query(t, "kitchen wiring"),
            "Instruct: retrieve notes\nQuery: kitchen wiring"
        );
    }

    #[test]
    fn last_token_index_skips_right_padding() {
        assert_eq!(last_token_index(&[1, 1, 1, 0, 0]), Some(2));
        assert_eq!(last_token_index(&[0, 0, 1, 1]), Some(3)); // left padding
        assert_eq!(last_token_index(&[0, 0, 0]), None);
    }

    #[test]
    fn last_token_pool_picks_the_final_real_token() {
        // 3 tokens × 2 dims; the 3rd is padding, so token #2 (index 1) should be chosen.
        let hidden = vec![
            1.0, 0.0, /* t0 */ 0.0, 9.0, /* t1 */ 5.0, 5.0, /* t2=pad */
        ];
        let pooled = pool(PoolingStrategy::LastToken, &hidden, 3, 2, &[1, 1, 0]);
        assert_eq!(pooled, vec![0.0, 9.0]);
    }

    #[test]
    fn mean_pool_averages_only_attended_tokens() {
        let hidden = vec![
            2.0, 0.0, /* t0 */ 4.0, 0.0, /* t1 */ 100.0, 100.0, /* pad */
        ];
        let pooled = pool(PoolingStrategy::Mean, &hidden, 3, 2, &[1, 1, 0]);
        assert_eq!(pooled, vec![3.0, 0.0]); // mean of t0,t1 only
    }

    #[test]
    fn inconsistent_shape_yields_empty() {
        assert!(pool(PoolingStrategy::Mean, &[1.0, 2.0, 3.0], 2, 2, &[1, 1]).is_empty());
        assert!(pool(PoolingStrategy::LastToken, &[], 0, 4, &[]).is_empty());
    }

    #[test]
    fn matryoshka_truncates_then_normalizes() {
        let e = matryoshka_embedding(vec![3.0, 4.0, 99.0, 99.0], Some(2));
        assert_eq!(e.len(), 2);
        assert!((e.magnitude() - 1.0).abs() < 1e-6);
        // 3,4 normalized → 0.6, 0.8
        assert!((e.0[0] - 0.6).abs() < 1e-6 && (e.0[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn matryoshka_keeps_full_vector_when_target_not_smaller() {
        let e = matryoshka_embedding(vec![1.0, 0.0, 0.0], Some(8));
        assert_eq!(e.len(), 3);
        let full = matryoshka_embedding(vec![1.0, 0.0, 0.0], None);
        assert_eq!(full.len(), 3);
    }
}
