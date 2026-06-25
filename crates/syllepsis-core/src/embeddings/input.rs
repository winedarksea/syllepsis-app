//! Token-budget policy for canonical full-note embeddings.

/// Combine already-tokenized prompt/title overhead with content tokens while staying within the
/// model context. Long content keeps equal-sized beginning and ending windows.
pub fn fit_document_token_ids(
    mut prefix_ids: Vec<i64>,
    content_ids: &[i64],
    max_context_tokens: usize,
    preserve_ending: bool,
) -> Vec<i64> {
    prefix_ids.truncate(max_context_tokens);
    let remaining = max_context_tokens.saturating_sub(prefix_ids.len());
    if content_ids.len() <= remaining {
        prefix_ids.extend_from_slice(content_ids);
        return prefix_ids;
    }
    if !preserve_ending {
        prefix_ids.extend_from_slice(&content_ids[..remaining]);
        return prefix_ids;
    }
    let head = remaining / 2;
    let tail = remaining - head;
    prefix_ids.extend_from_slice(&content_ids[..head]);
    prefix_ids.extend_from_slice(&content_ids[content_ids.len() - tail..]);
    prefix_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_is_not_duplicated() {
        assert_eq!(
            fit_document_token_ids(vec![9], &[1, 2, 3], 8, true),
            vec![9, 1, 2, 3]
        );
    }

    #[test]
    fn long_content_keeps_beginning_and_end_within_limit() {
        let content = (0..20).collect::<Vec<_>>();
        let fitted = fit_document_token_ids(vec![99, 98], &content, 10, true);
        assert_eq!(fitted, vec![99, 98, 0, 1, 2, 3, 16, 17, 18, 19]);
        assert_eq!(fitted.len(), 10);
    }

    #[test]
    fn prompt_overhead_can_consume_entire_context() {
        assert_eq!(
            fit_document_token_ids(vec![1, 2, 3, 4], &[5, 6], 3, true),
            vec![1, 2, 3]
        );
    }
}
