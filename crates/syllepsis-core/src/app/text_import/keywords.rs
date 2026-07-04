//! Deterministic keyword-based category suggestions over a text-import preview.
//!
//! Pure frequency analysis (no book access, no model): tokenize titles and bodies, score
//! unigrams and adjacent-word bigrams by weighted occurrence × document spread, and return the
//! top terms with the preview items they would apply to. Whether a suggestion already exists as
//! a category is a frontend concern (it has the store's category list).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::id::slugify;

use super::outline::SPLIT_NOTE_MARKER;
use super::TextImportPreviewItem;

const TITLE_WEIGHT: f32 = 3.0;
const BODY_WEIGHT: f32 = 1.0;
const BIGRAM_WEIGHT: f32 = 2.0;
/// A term must appear in at least this many distinct items to be suggested.
const MIN_DOC_FREQUENCY: usize = 3;
pub const DEFAULT_MAX_SUGGESTIONS: usize = 12;

/// Import-scoped stopword list: common English plus the advice/planning fillers that dominate
/// note documents ("should", "would", "make", "good"...). Deliberately separate from the slug
/// stopwords in `id.rs`, which serve a different purpose (short readable ids).
const STOPWORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "are",
    "but",
    "not",
    "you",
    "all",
    "any",
    "can",
    "had",
    "her",
    "was",
    "one",
    "our",
    "out",
    "day",
    "get",
    "has",
    "him",
    "his",
    "how",
    "man",
    "new",
    "now",
    "old",
    "see",
    "two",
    "way",
    "who",
    "boy",
    "did",
    "its",
    "let",
    "put",
    "say",
    "she",
    "too",
    "use",
    "that",
    "with",
    "have",
    "this",
    "will",
    "your",
    "from",
    "they",
    "know",
    "want",
    "been",
    "much",
    "some",
    "time",
    "very",
    "when",
    "come",
    "here",
    "just",
    "like",
    "long",
    "many",
    "more",
    "most",
    "over",
    "such",
    "take",
    "than",
    "them",
    "well",
    "were",
    "what",
    "which",
    "while",
    "would",
    "should",
    "could",
    "might",
    "must",
    "shall",
    "make",
    "makes",
    "making",
    "made",
    "good",
    "best",
    "better",
    "great",
    "really",
    "thing",
    "things",
    "something",
    "anything",
    "everything",
    "nothing",
    "also",
    "only",
    "then",
    "there",
    "these",
    "those",
    "into",
    "onto",
    "about",
    "after",
    "before",
    "between",
    "under",
    "above",
    "below",
    "each",
    "other",
    "another",
    "same",
    "still",
    "even",
    "ever",
    "never",
    "always",
    "often",
    "maybe",
    "probably",
    "perhaps",
    "around",
    "through",
    "where",
    "why",
    "because",
    "since",
    "until",
    "unless",
    "though",
    "although",
    "however",
    "either",
    "neither",
    "both",
    "few",
    "less",
    "least",
    "own",
    "off",
    "once",
    "again",
    "further",
    "doing",
    "does",
    "done",
    "being",
    "having",
    "need",
    "needs",
    "needed",
    "used",
    "using",
    "uses",
    "lots",
    "lot",
    "bit",
    "kind",
    "sort",
    "type",
    "part",
    "side",
    "back",
    "front",
    "near",
    "next",
    "first",
    "last",
    "small",
    "large",
    "big",
    "little",
    "high",
    "low",
    "top",
    "bottom",
    "right",
    "left",
    "don",
    "won",
    "isn",
    "aren",
    "wasn",
    "doesn",
    "didn",
    "haven",
    "hasn",
    "wouldn",
    "shouldn",
    "couldn",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextImportCategorySuggestion {
    /// Slug usable directly as a category name.
    pub name: String,
    /// Human-readable form of the term.
    pub label: String,
    pub score: f32,
    /// Preview item indices (positions in the given slice) the term appears in.
    pub item_indices: Vec<usize>,
}

#[derive(Debug, Default)]
struct TermStats {
    label: String,
    weight: f32,
    item_indices: Vec<usize>,
    is_bigram: bool,
}

pub fn suggest_categories(
    items: &[TextImportPreviewItem],
    max: usize,
) -> Vec<TextImportCategorySuggestion> {
    let mut stats: HashMap<String, TermStats> = HashMap::new();

    for (item_index, item) in items.iter().enumerate() {
        accumulate_text(&mut stats, &item.title, TITLE_WEIGHT, item_index);
        accumulate_text(&mut stats, &item.body, BODY_WEIGHT, item_index);
    }

    let mut scored: Vec<(String, TermStats, f32)> = stats
        .into_iter()
        .filter(|(_, term)| term.item_indices.len() >= MIN_DOC_FREQUENCY)
        .map(|(key, term)| {
            let df = term.item_indices.len() as f32;
            let score = term.weight * (1.0 + df).log2();
            (key, term, score)
        })
        .collect();

    // Absorb a unigram into any higher-scoring bigram that contains it.
    let bigram_scores: Vec<(String, f32)> = scored
        .iter()
        .filter(|(_, term, _)| term.is_bigram)
        .map(|(key, _, score)| (key.clone(), *score))
        .collect();
    scored.retain(|(key, term, score)| {
        if term.is_bigram {
            return true;
        }
        !bigram_scores.iter().any(|(bigram, bigram_score)| {
            *bigram_score >= *score && bigram_contains_word(bigram, key)
        })
    });

    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(if max == 0 {
            DEFAULT_MAX_SUGGESTIONS
        } else {
            max
        })
        .map(|(key, term, score)| TextImportCategorySuggestion {
            name: slug_for_term(&key),
            label: term.label,
            score,
            item_indices: term.item_indices,
        })
        .collect()
}

fn bigram_contains_word(bigram: &str, word: &str) -> bool {
    bigram.split(' ').any(|part| part == word)
}

fn slug_for_term(term: &str) -> String {
    let slug = slugify(term);
    if slug.is_empty() {
        term.replace(' ', "-")
    } else {
        slug
    }
}

fn accumulate_text(
    stats: &mut HashMap<String, TermStats>,
    text: &str,
    weight: f32,
    item_index: usize,
) {
    let words = tokenize(text);
    for word in &words {
        add_term(stats, word.clone(), word.clone(), weight, item_index, false);
    }
    for pair in words.windows(2) {
        let key = format!("{} {}", pair[0], pair[1]);
        add_term(
            stats,
            key.clone(),
            key,
            weight * BIGRAM_WEIGHT,
            item_index,
            true,
        );
    }
}

fn add_term(
    stats: &mut HashMap<String, TermStats>,
    key: String,
    label: String,
    weight: f32,
    item_index: usize,
    is_bigram: bool,
) {
    let entry = stats.entry(key).or_insert_with(|| TermStats {
        label,
        weight: 0.0,
        item_indices: Vec::new(),
        is_bigram,
    });
    entry.weight += weight;
    if entry.item_indices.last() != Some(&item_index) {
        entry.item_indices.push(item_index);
    }
}

/// Lowercased content words: punctuation/markdown stripped, stopwords, short tokens, and
/// numerics dropped. Word order is preserved so adjacent pairs form meaningful bigrams.
fn tokenize(text: &str) -> Vec<String> {
    let text = text.replace(SPLIT_NOTE_MARKER, " ");
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|raw| raw.trim_matches('\'').to_lowercase().replace('\'', ""))
        .filter(|word| {
            word.chars().count() >= 3
                && !word.chars().all(|c| c.is_ascii_digit())
                && !STOPWORDS.contains(&word.as_str())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{TextImportBlockKind, TextImportPreviewItem};
    use super::*;

    fn item(index: usize, title: &str, body: &str) -> TextImportPreviewItem {
        TextImportPreviewItem {
            index,
            title: title.to_string(),
            body: body.to_string(),
            block_kind: TextImportBlockKind::Paragraph,
            category_context: None,
            intended_prior: None,
            warnings: Vec::new(),
            categories: Vec::new(),
            parent_index: None,
            depth: 0,
        }
    }

    #[test]
    fn stopwords_and_short_or_numeric_tokens_are_dropped() {
        let items: Vec<_> = (0..3)
            .map(|i| {
                item(
                    i,
                    "You should make it good",
                    "The best thing would be 42 of them",
                )
            })
            .collect();
        assert!(suggest_categories(&items, 12).is_empty());
    }

    #[test]
    fn split_note_marker_does_not_pollute_suggestions() {
        // The outline parser's own "(split into its own note)" marker bullet is app-generated,
        // not source text, and must not surface as a keyword suggestion (e.g. "split-note").
        let items: Vec<_> = (0..5)
            .map(|i| item(i, "", &format!("- some outline heading{SPLIT_NOTE_MARKER}")))
            .collect();
        let suggestions = suggest_categories(&items, 12);
        assert!(
            suggestions.iter().all(|s| s.name != "split-note"),
            "marker text leaked into suggestions: {suggestions:?}"
        );
    }

    #[test]
    fn title_terms_outscore_body_terms() {
        // Distinct neighbor words keep bigrams below the df threshold so the unigrams compete
        // directly: one title occurrence (weight 3) vs two body occurrences (weight 1 each).
        let neighbors = [("alpha", "beta"), ("gamma", "delta"), ("epsilon", "zeta")];
        let items: Vec<_> = neighbors
            .iter()
            .enumerate()
            .map(|(i, (a, b))| item(i, "concrete", &format!("insulation {a} insulation {b}")))
            .collect();
        let suggestions = suggest_categories(&items, 12);
        let concrete = suggestions
            .iter()
            .find(|s| s.name == "concrete")
            .expect("concrete suggested");
        let insulation = suggestions
            .iter()
            .find(|s| s.name == "insulation")
            .expect("insulation suggested");
        assert!(
            concrete.score > insulation.score,
            "title weight beats double body occurrence: {} vs {}",
            concrete.score,
            insulation.score
        );
    }

    #[test]
    fn adjacent_word_bigrams_surface_and_absorb_their_unigrams() {
        let items: Vec<_> = (0..4)
            .map(|i| item(i, "Hot tub planning", "The hot tub needs a hot tub cover"))
            .collect();
        let suggestions = suggest_categories(&items, 12);
        let names: Vec<&str> = suggestions.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"hot-tub"), "bigram surfaces: {names:?}");
        assert!(
            !names.contains(&"hot") && !names.contains(&"tub"),
            "unigrams absorbed by the bigram: {names:?}"
        );
    }

    #[test]
    fn terms_below_document_frequency_threshold_are_dropped() {
        let mut items = vec![
            item(0, "concrete pour", "concrete"),
            item(1, "concrete forms", "concrete"),
        ];
        assert!(suggest_categories(&items, 12)
            .iter()
            .all(|s| s.name != "concrete"));
        items.push(item(2, "concrete cure", "concrete"));
        let suggestions = suggest_categories(&items, 12);
        let concrete = suggestions
            .iter()
            .find(|s| s.name == "concrete")
            .expect("df of 3 passes");
        assert_eq!(concrete.item_indices, vec![0, 1, 2]);
    }

    #[test]
    fn max_limits_the_suggestion_count() {
        let words = ["alpha", "bravo", "charlie", "delta", "echo"];
        let items: Vec<_> = (0..3)
            .map(|i| {
                item(
                    i,
                    "",
                    &words
                        .iter()
                        .map(|w| format!("{w} filler"))
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })
            .collect();
        let suggestions = suggest_categories(&items, 2);
        assert!(suggestions.len() <= 2);
    }
}
