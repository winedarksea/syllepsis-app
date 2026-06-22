//! The one shared tokenizer used by both the embedding pipeline and the lexical search index.
//!
//! Keeping a single definition here means a note is split into the *same* terms whether it is
//! being hashed into a vector ([`crate::embeddings`]) or indexed for BM25 ([`crate::search`]).
//! If the two disagreed, vector and keyword search would quietly rank against different views
//! of the text. It lives outside both modules so neither has to depend on the other.

use std::sync::LazyLock;

use regex::Regex;

use crate::markdown::dialect;

/// Runs of alphanumerics (Unicode-aware). Splitting on everything else naturally drops the
/// dialect sigils — `#kitchen` → `kitchen`, `@aristotle` → `aristotle`, `loc:firstfloor` →
/// `loc`, `firstfloor` — so the tokens carry the words, not the punctuation.
static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+").unwrap());

/// Lowercased word tokens of `text`, with `%%comment%%` spans removed first (an author's
/// private margin notes should not affect search or similarity).
pub fn tokenize(text: &str) -> Vec<String> {
    let cleaned = dialect::strip_comments(text);
    WORD_RE
        .find_iter(&cleaned)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_splits_on_punctuation() {
        assert_eq!(
            tokenize("Wire the #Kitchen, then the Main-Panel."),
            vec!["wire", "the", "kitchen", "then", "the", "main", "panel"]
        );
    }

    #[test]
    fn drops_comment_spans() {
        assert_eq!(
            tokenize("keep %%secret aside%% words"),
            vec!["keep", "words"]
        );
    }

    #[test]
    fn empty_when_no_word_characters() {
        assert!(tokenize("%%all hidden%%  --- ").is_empty());
    }
}
