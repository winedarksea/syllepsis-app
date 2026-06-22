//! Exact substring matching — the "I typed it, find that literal phrase" ranking.
//!
//! BM25 tokenizes and ignores order, so a search for a specific multi-word phrase, an id
//! fragment, or a code snippet can rank a document that merely shares the individual words
//! above the one containing the literal string. Exact match is the corrective: it ranks only
//! documents that contain the query as a contiguous (case-insensitive) substring, by how many
//! times it occurs. Fused with BM25 and vector via RRF, it reliably floats literal hits up.

/// Rank documents containing `query` as a case-insensitive substring, by occurrence count,
/// descending. Documents without the substring are omitted. A blank query matches nothing.
pub fn match_exact(documents: &[String], query: &str) -> Vec<(usize, f32)> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut ranked: Vec<(usize, f32)> = documents
        .iter()
        .enumerate()
        .filter_map(|(i, doc)| {
            let count = doc.to_lowercase().matches(&needle).count();
            (count > 0).then_some((i, count as f32))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Vec<String> {
        vec![
            "the breaker panel is in the kitchen".into(),
            "panel breaker discussion without the phrase".into(),
            "the breaker panel appears here and the breaker panel again".into(),
        ]
    }

    #[test]
    fn matches_contiguous_phrase_case_insensitively() {
        let ranked = match_exact(&docs(), "Breaker Panel");
        let ids: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        // doc 1 has the words but not the contiguous phrase → excluded.
        assert!(ids.contains(&0) && ids.contains(&2));
        assert!(!ids.contains(&1));
    }

    #[test]
    fn ranks_by_occurrence_count() {
        let ranked = match_exact(&docs(), "breaker panel");
        assert_eq!(ranked[0].0, 2, "doc 2 contains the phrase twice");
    }

    #[test]
    fn blank_query_matches_nothing() {
        assert!(match_exact(&docs(), "   ").is_empty());
    }
}
