//! Reciprocal Rank Fusion: combining several rankings into one.
//!
//! Exact, BM25, and vector search each return their own ordering of notes on incomparable
//! scales (a BM25 score and a cosine similarity cannot be added directly). RRF sidesteps the
//! scale problem by using only **rank position**: a note's fused score is the sum over each
//! ranking of `1 / (k + rank)`. A note near the top of several lists rises; the constant `k`
//! (config `rrf_k`, default 60) damps the influence of the very top positions so one list
//! cannot wholly dominate. This is the same fusion FTS5+vector hybrid search uses.

use std::collections::HashMap;
use std::hash::Hash;

use crate::config::SearchConfig;

/// Fuse several rankings (each a list of ids in best-first order) into one descending ranking
/// of `(id, fused_score)`. Ids may appear in any subset of the input rankings. Ties break by
/// score then by first appearance, giving a deterministic order.
pub fn reciprocal_rank_fusion<Id>(rankings: &[Vec<Id>], cfg: &SearchConfig) -> Vec<(Id, f32)>
where
    Id: Clone + Eq + Hash,
{
    let k = cfg.rrf_k;
    let mut scores: HashMap<Id, f32> = HashMap::new();
    let mut first_seen: HashMap<Id, usize> = HashMap::new();
    let mut order = 0usize;

    for ranking in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
            first_seen.entry(id.clone()).or_insert_with(|| {
                let o = order;
                order += 1;
                o
            });
        }
    }

    let mut fused: Vec<(Id, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| first_seen[&a.0].cmp(&first_seen[&b.0]))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement_across_lists_wins() {
        // "b" is rank 0 in both lists; nothing else tops it in more than one.
        let lists = vec![vec!["b", "a", "c"], vec!["b", "c", "a"]];
        let fused = reciprocal_rank_fusion(&lists, &SearchConfig::default());
        assert_eq!(fused[0].0, "b");
    }

    #[test]
    fn an_id_in_one_list_still_appears() {
        let lists = vec![vec!["a", "b"], vec!["c"]];
        let fused = reciprocal_rank_fusion(&lists, &SearchConfig::default());
        let ids: Vec<&str> = fused.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b") && ids.contains(&"c"));
    }

    #[test]
    fn higher_rank_contributes_more() {
        // Same single list; the top item must outscore the rest.
        let lists = vec![vec!["first", "second", "third"]];
        let fused = reciprocal_rank_fusion(&lists, &SearchConfig::default());
        assert_eq!(fused[0].0, "first");
        assert!(fused[0].1 > fused[1].1);
    }

    #[test]
    fn empty_input_is_empty_output() {
        let lists: Vec<Vec<&str>> = vec![];
        assert!(reciprocal_rank_fusion(&lists, &SearchConfig::default()).is_empty());
    }
}
