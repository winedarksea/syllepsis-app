//! The Syllepsis markdown dialect: the inline tokens layered on top of CommonMark.
//!
//! Phase 1 ships the **full token grammar** so the on-disk format is stable from the start,
//! even though some tokens are only acted on later (e.g. `loc:` picking in the spatial phase,
//! cloze study mode in the AI phase). This module is pure parsing — no rendering — exposing
//! the extraction the rest of the crate needs now: categories (`#tag`), references (`@ref`),
//! locations (`loc:`), cloze/spoiler spans (`||..||`), and comment stripping (`%%..%%`).

use std::sync::LazyLock;

use regex::Regex;

// Compiled once. A leading boundary (start-of-string or whitespace) avoids matching `#`/`@`
// inside words, URLs, or fragments like `a#b`.
static CATEGORY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)#([A-Za-z0-9][\w-]*)").unwrap());
static REFERENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)@([A-Za-z0-9][\w-]*)").unwrap());
static LOCATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"loc:(\S+)").unwrap());
static COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)%%.*?%%").unwrap());
static CLOZE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|\|(.+?)\|\|").unwrap());

/// A cloze deletion / spoiler span `||hidden||`, optionally with a group and a hint:
/// `||c1::hidden|hint||`. The group lets multiple spans reveal together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloze {
    /// The hidden text.
    pub hidden: String,
    /// Optional hint shown in place of the blank.
    pub hint: Option<String>,
    /// Optional group id (`c1`) so co-grouped spans reveal as one.
    pub group: Option<String>,
}

/// Extract unique inline `#category` names, in first-seen order.
pub fn extract_categories(text: &str) -> Vec<String> {
    unique_captures(&CATEGORY_RE, text)
}

/// Extract unique inline `@reference` ids, in first-seen order.
pub fn extract_references(text: &str) -> Vec<String> {
    unique_captures(&REFERENCE_RE, text)
}

/// Extract raw `loc:` location tokens (the value after `loc:`), in first-seen order.
/// Resolution into worlds/coordinates happens in the spatial phase.
pub fn extract_locations(text: &str) -> Vec<String> {
    unique_captures(&LOCATION_RE, text)
}

/// Parse all cloze/spoiler spans in document order.
pub fn parse_clozes(text: &str) -> Vec<Cloze> {
    CLOZE_RE
        .captures_iter(text)
        .map(|c| parse_cloze_inner(&c[1]))
        .collect()
}

/// Remove `%%comment%%` spans (including multi-line) from rendered output.
pub fn strip_comments(text: &str) -> String {
    COMMENT_RE.replace_all(text, "").into_owned()
}

/// Parse the inside of a `||..||` span into its group / hidden / hint parts.
fn parse_cloze_inner(inner: &str) -> Cloze {
    // Optional `group::` prefix.
    let (group, remainder) = match inner.split_once("::") {
        Some((g, rest)) if is_group_id(g) => (Some(g.to_string()), rest),
        _ => (None, inner),
    };
    // Optional `|hint` suffix.
    let (hidden, hint) = match remainder.split_once('|') {
        Some((h, hint)) => (h.to_string(), Some(hint.to_string())),
        None => (remainder.to_string(), None),
    };
    Cloze {
        hidden,
        hint,
        group,
    }
}

/// A group id is a short alphanumeric/underscore token (e.g. `c1`); anything else means the
/// `::` was part of the hidden content, not a group prefix.
fn is_group_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Collect capture group 1 across all matches, de-duplicated, preserving first-seen order.
fn unique_captures(re: &Regex, text: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for cap in re.captures_iter(text) {
        let value = cap[1].to_string();
        if !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_categories_not_headings_or_fragments() {
        let text = "Wire the #kitchen and #main-panel. See ref note-x#section (not a tag).";
        assert_eq!(extract_categories(text), vec!["kitchen", "main-panel"]);
    }

    #[test]
    fn extracts_references_and_locations() {
        assert_eq!(
            extract_references("As @montaigne argued, per @aristotle."),
            vec!["montaigne", "aristotle"]
        );
        assert_eq!(
            extract_locations("Meet at loc:47.6062,-122.3321 then loc:firstfloor/0.42,0.31"),
            vec!["47.6062,-122.3321", "firstfloor/0.42,0.31"]
        );
    }

    #[test]
    fn parses_cloze_variants() {
        assert_eq!(
            parse_clozes("The capital is ||Paris||."),
            vec![Cloze {
                hidden: "Paris".into(),
                hint: None,
                group: None
            }]
        );
        assert_eq!(
            parse_clozes("||c1::mitochondria|the powerhouse||"),
            vec![Cloze {
                hidden: "mitochondria".into(),
                hint: Some("the powerhouse".into()),
                group: Some("c1".into())
            }]
        );
        // A bare `||hidden|hint||` with no group.
        assert_eq!(
            parse_clozes("||answer|a hint||")[0],
            Cloze {
                hidden: "answer".into(),
                hint: Some("a hint".into()),
                group: None
            }
        );
    }

    #[test]
    fn strips_inline_and_multiline_comments() {
        assert_eq!(strip_comments("keep %%drop this%% keep"), "keep  keep");
        assert_eq!(strip_comments("a %%multi\nline%% b"), "a  b");
    }
}
