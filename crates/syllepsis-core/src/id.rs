//! Note identity: the `{type}-{slug}-{ulid}` scheme.
//!
//! Design rules from object-types.md:
//! - The **ulid** is the canonical, immutable identity. Time-ordered (files sort
//!   chronologically) and high-entropy (offline devices and imported packs never collide).
//! - The **type** prefix is cheap human context; the **slug** is cosmetic and *mutable*
//!   (it regenerates when the title changes).
//! - No colons anywhere, so the same string is a safe filename on every platform.
//! - **Lookups resolve on the ulid tail**, so a stale slug still resolves and renames are
//!   safe. See [`NoteId::same_identity`].
//!
//! The canonical id string lives in a note's frontmatter, never in its path — so a note can
//! move between sorting subfolders or be renamed externally without losing identity.

use std::sync::{LazyLock, Mutex};

use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use ulid::{Generator, Ulid};

/// Process-global **monotonic** ULID generator. Plain `Ulid::new()` uses random entropy, so
/// two ids minted in the same millisecond order randomly — but the design relies on ids being
/// chronological (sibling notes sort by creation order; files sort by time). A monotonic
/// generator increments the random component within a millisecond, so ids minted in sequence
/// are strictly increasing within a session and timestamp-ordered across sessions.
static MONOTONIC: LazyLock<Mutex<Generator>> = LazyLock::new(|| Mutex::new(Generator::new()));

/// Mint the next monotonic ulid as a lowercased string. Falls back to a random ulid only on
/// the astronomically rare monotonic overflow (>2^80 ids in one millisecond) or a poisoned
/// lock — never a silent heuristic, just a guaranteed-valid id.
fn next_ulid() -> String {
    let ulid = MONOTONIC
        .lock()
        .map(|mut gen| gen.generate().unwrap_or_else(|_| Ulid::new()))
        .unwrap_or_else(|_| Ulid::new());
    ulid.to_string().to_lowercase()
}

/// Max slug length before truncation (truncated at a token boundary where possible).
const SLUG_MAX_LEN: usize = 32;

/// Length of a Crockford base32 ULID in characters.
const ULID_LEN: usize = 26;

/// Common English stopwords trimmed from slugs. Kept short on purpose — the slug is
/// cosmetic, so over-aggressive filtering would only hurt readability.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "of", "on", "or", "the", "to", "with",
];

/// A parsed, validated note id. Stored as the full canonical string; the components are
/// derived on demand so the struct stays a thin newtype that serializes as a plain string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NoteId(String);

impl NoteId {
    /// Mint a brand-new id with a fresh, time-ordered ulid.
    ///
    /// `type_prefix` is the object type's id prefix (e.g. `"note"`, `"quote"`); pass it from
    /// [`crate::model::ObjectType`] so this module stays free of a model dependency.
    pub fn generate(type_prefix: &str, title: &str) -> NoteId {
        Self::with_ulid(type_prefix, title, &next_ulid())
    }

    /// Construct an id from an explicit ulid — used for knowledge-pack re-import (match the
    /// existing identity), forking (a *new* ulid), and the registry's collision-regen path.
    pub fn with_ulid(type_prefix: &str, title: &str, ulid: &str) -> NoteId {
        let slug = slugify(title);
        let id = if slug.is_empty() {
            format!("{type_prefix}-{ulid}")
        } else {
            format!("{type_prefix}-{slug}-{ulid}")
        };
        NoteId(id)
    }

    /// Parse and validate an existing id string. Validation hinges on the trailing ulid: the
    /// type is everything before the first `-`, the ulid is the final 26-char Crockford
    /// segment, and the slug is whatever sits between (it may itself contain hyphens).
    pub fn parse(s: &str) -> CoreResult<NoteId> {
        if s.contains(':') {
            return Err(CoreError::InvalidId(format!("contains colon: {s}")));
        }
        let (type_prefix, rest) = s
            .split_once('-')
            .ok_or_else(|| CoreError::InvalidId(format!("missing type prefix: {s}")))?;
        if type_prefix.is_empty() {
            return Err(CoreError::InvalidId(format!("empty type prefix: {s}")));
        }

        // The ulid is the final hyphen-delimited segment (no slug) or, when a slug is
        // present, the tail after the last hyphen.
        let ulid_candidate = rest.rsplit('-').next().unwrap_or(rest);
        if !is_valid_ulid(ulid_candidate) {
            return Err(CoreError::InvalidId(format!("invalid ulid tail: {s}")));
        }
        Ok(NoteId(s.to_string()))
    }

    /// The full canonical id string (what is stored in frontmatter and used as a filename).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The object-type prefix (`"note"`, `"quote"`, …).
    pub fn type_prefix(&self) -> &str {
        self.0.split_once('-').map(|(t, _)| t).unwrap_or(&self.0)
    }

    /// The canonical, immutable ulid tail. All resolution happens against this.
    pub fn ulid(&self) -> &str {
        // Safe: any `NoteId` was validated to end in a ulid segment.
        self.0.rsplit('-').next().unwrap_or(&self.0)
    }

    /// The cosmetic slug between type and ulid (empty when the title produced no slug).
    pub fn slug(&self) -> &str {
        let after_type = match self.0.split_once('-') {
            Some((_, rest)) => rest,
            None => return "",
        };
        match after_type.rsplit_once('-') {
            Some((slug, _ulid)) => slug,
            None => "", // `type-ulid`, no slug
        }
    }

    /// Two ids refer to the same note iff their ulids match — the title/slug may have drifted.
    pub fn same_identity(&self, other: &NoteId) -> bool {
        self.ulid() == other.ulid()
    }

    /// Produce a copy with the slug regenerated from a new title, preserving type and ulid.
    /// This is how a title rename keeps identity while refreshing the cosmetic slug.
    pub fn with_regenerated_slug(&self, new_title: &str) -> NoteId {
        NoteId::with_ulid(self.type_prefix(), new_title, self.ulid())
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for NoteId {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        NoteId::parse(&value)
    }
}

impl From<NoteId> for String {
    fn from(value: NoteId) -> String {
        value.0
    }
}

/// True if `s` is a syntactically valid lowercase Crockford-base32 ULID.
fn is_valid_ulid(s: &str) -> bool {
    s.len() == ULID_LEN
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        // Round-trip through the ulid crate to enforce the Crockford alphabet/overflow rules.
        && Ulid::from_string(&s.to_uppercase()).is_ok()
}

/// Derive a cosmetic slug from a title: ASCII-fold, lowercase, kebab-case, drop stopwords,
/// truncate to [`SLUG_MAX_LEN`] at a token boundary.
pub fn slugify(title: &str) -> String {
    let folded = ascii_fold(title);
    let tokens: Vec<String> = folded
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect();
    if tokens.is_empty() {
        return String::new();
    }

    // Drop stopwords, but fall back to the full token list if filtering empties it (e.g. a
    // title that is entirely stopwords still deserves *some* slug).
    let kept: Vec<&String> = tokens
        .iter()
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect();
    let source: Vec<&String> = if kept.is_empty() {
        tokens.iter().collect()
    } else {
        kept
    };

    // Greedily join tokens while under the length budget so we cut on word boundaries.
    let mut slug = String::new();
    for token in source {
        let candidate_len = if slug.is_empty() {
            token.len()
        } else {
            slug.len() + 1 + token.len()
        };
        if candidate_len > SLUG_MAX_LEN {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(token);
    }

    // A single leading token longer than the budget gets hard-truncated.
    if slug.is_empty() {
        if let Some(first) = source_first(title) {
            slug = first.chars().take(SLUG_MAX_LEN).collect();
        }
    }
    slug
}

/// Recompute the first ascii-folded token of a title (used only for the hard-truncate path).
fn source_first(title: &str) -> Option<String> {
    ascii_fold(title)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
}

/// Transliterate common accented Latin characters to ASCII and drop the rest. This is a
/// pragmatic POC fold (not full Unicode NFKD): enough to keep most Western titles readable
/// in the cosmetic slug without pulling in a transliteration crate.
fn ascii_fold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => out.push('a'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => out.push('i'),
            'ñ' | 'ń' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            c if c.is_ascii() => out.push(c),
            // Non-mappable, non-ascii: emit a separator so adjacent words don't fuse.
            _ => out.push(' '),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_parses_round_trip() {
        let id = NoteId::generate("quote", "Montaigne on Friendship");
        let parsed = NoteId::parse(id.as_str()).expect("freshly generated id must parse");
        assert_eq!(id, parsed);
        assert_eq!(id.type_prefix(), "quote");
        assert_eq!(id.slug(), "montaigne-friendship"); // "on" is a stopword
        assert_eq!(id.ulid().len(), ULID_LEN);
    }

    #[test]
    fn ulid_tail_drives_identity_across_slug_drift() {
        let id = NoteId::generate("note", "First Title");
        let renamed = id.with_regenerated_slug("A Completely Different Heading");
        assert_ne!(id.slug(), renamed.slug());
        assert_eq!(id.ulid(), renamed.ulid());
        assert!(id.same_identity(&renamed));
    }

    #[test]
    fn parses_id_with_hyphenated_slug() {
        let id = NoteId::generate("reference", "well-tempered clavier");
        let parsed = NoteId::parse(id.as_str()).unwrap();
        assert_eq!(parsed.slug(), "well-tempered-clavier");
        assert_eq!(parsed.type_prefix(), "reference");
    }

    #[test]
    fn handles_empty_slug() {
        let id = NoteId::generate("note", "...!!!"); // no alphanumerics
        assert_eq!(id.slug(), "");
        assert!(NoteId::parse(id.as_str()).is_ok());
        assert_eq!(id.type_prefix(), "note");
    }

    #[test]
    fn rejects_colons_and_bad_ulids() {
        assert!(NoteId::parse("note-title-bad").is_err());
        assert!(NoteId::parse("note:title-01jh5k3q2x9y8w7v6t5s4r3q2p").is_err());
        assert!(NoteId::parse("01jh5k3q2x9y8w7v6t5s4r3q2p").is_err()); // no type prefix
    }

    #[test]
    fn slug_truncates_at_token_boundary() {
        let slug = slugify("supercalifragilistic expialidocious wonderful amazing journey");
        assert!(slug.len() <= SLUG_MAX_LEN);
        // Must not cut mid-token: the slug ends on a complete word.
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn ascii_folds_accents() {
        let id = NoteId::generate("note", "Café Montréal");
        assert_eq!(id.slug(), "cafe-montreal");
    }
}
