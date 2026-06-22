//! The full per-note metadata bundle stored in frontmatter.
//!
//! This file ships the **complete** schema in Phase 1 even though some fields are only
//! acted on later (privacy/lock in Phase 6, packs in Phase 6, fork in Phase 3). That is the
//! "build files near-final" rule: the on-disk format is stable from the first commit, and
//! later phases add behavior that reads fields already present here.
//!
//! High-churn analytics (open counts, LLM-retrieval counts) deliberately do **not** live
//! here — they would rewrite frontmatter constantly and create CRDT/sync noise, so they
//! belong in the `_derived/` SQLite cache (Phase 6), not the source-of-truth markdown.

use crate::id::NoteId;
use crate::model::classification::Classification;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Skip-serializing helper so default sub-sections stay out of the frontmatter, keeping
/// files clean when read outside the app.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// A date that is either absolute or expressed relative to another note's date (`+N days`).
/// Resolution of the relative form happens at render time against the anchor note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FlexDate {
    /// Absolute date, when known/pinned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    /// Anchor note for a relative date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_to: Option<NoteId>,
    /// Offset in days from the anchor (may be negative).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_days: Option<i64>,
    /// Flag this date as a reminder.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reminder: bool,
}

/// System-tracked and user-optional dates. `created`/`updated` are always present and
/// managed by the app; `scheduled`/`completed` are user-set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateMetadata {
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<FlexDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<FlexDate>,
}

impl DateMetadata {
    /// Fresh dates for a note created now (created == updated).
    pub fn now() -> Self {
        let now = Utc::now();
        DateMetadata {
            created: now,
            updated: now,
            scheduled: None,
            completed: None,
        }
    }
}

impl Default for DateMetadata {
    fn default() -> Self {
        DateMetadata {
            created: DateTime::<Utc>::UNIX_EPOCH,
            updated: DateTime::<Utc>::UNIX_EPOCH,
            scheduled: None,
            completed: None,
        }
    }
}

/// Lightweight, note-level (not line-level) authorship tracking tied to the cloud identity
/// provider (GitHub/Google handle); `ownership` is the one field users reassign in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Authorship {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edited_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<String>,
    /// True when the note's content was produced by an AI rather than a human.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ai_generated: bool,
}

/// Records that this note is a fork of another (forking mints a *new* ulid; this points back).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkInfo {
    pub forked_from: NoteId,
    pub forked_at: DateTime<Utc>,
}

/// Locking mode for self-protection (privacy-security.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockMode {
    /// Freely editable.
    #[default]
    None,
    /// Proposed rewrites must wait the configured delay before merging.
    UnlockDelay,
    /// A passing fact-check is required before a rewrite can merge.
    FactCheckGate,
}

/// Privacy, locking, archival, and deletion lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Lifecycle {
    /// Excluded from the GitHub publish (gitignore) and from RAG/default views.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub lock: LockMode,
    /// Hidden from RAG and default views, but toggleable back on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    /// "Vanishing note": self-deletes at this time (set at creation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanish_at: Option<DateTime<Utc>>,
    /// Start of the deletion-delay window ("mark for deletion").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marked_for_deletion_at: Option<DateTime<Utc>>,
}

/// Knowledge-pack membership. A note may belong to multiple packs; `locally_modified`
/// protects user edits from being overwritten on a pack version re-import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PackMembership {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_version: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locally_modified: bool,
}

/// Secondary kanban/scrum fields (lower-priority feature surface).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Kanban {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<u32>,
}

/// The complete metadata bundle embedded in a [`super::Note`]. Default sections are skipped
/// on serialize so a plain note's frontmatter stays minimal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Metadata {
    #[serde(skip_serializing_if = "is_default")]
    pub classification: Classification,
    pub dates: DateMetadata,
    #[serde(skip_serializing_if = "is_default")]
    pub authorship: Authorship,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork: Option<ForkInfo>,
    #[serde(skip_serializing_if = "is_default")]
    pub lifecycle: Lifecycle,
    #[serde(skip_serializing_if = "is_default")]
    pub packs: PackMembership,
    #[serde(skip_serializing_if = "is_default")]
    pub kanban: Kanban,
}

impl Metadata {
    /// Metadata for a note created now.
    pub fn now() -> Self {
        Metadata {
            dates: DateMetadata::now(),
            ..Default::default()
        }
    }

    /// Whether the note is currently hidden from RAG/default views (private or archived).
    pub fn is_hidden_from_default_views(&self) -> bool {
        self.lifecycle.private || self.lifecycle.archived
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_note_serializes_only_dates() {
        let meta = Metadata::now();
        let yaml = serde_yaml::to_string(&meta).unwrap();
        // Default sub-sections are skipped; only the always-present dates remain.
        assert!(yaml.contains("dates:"));
        assert!(!yaml.contains("classification:"));
        assert!(!yaml.contains("kanban:"));
        assert!(!yaml.contains("lifecycle:"));
    }

    #[test]
    fn round_trips_with_lifecycle_and_fork() {
        let mut meta = Metadata::now();
        meta.lifecycle.private = true;
        meta.lifecycle.lock = LockMode::UnlockDelay;
        meta.fork = Some(ForkInfo {
            forked_from: NoteId::generate("note", "parent"),
            forked_at: Utc::now(),
        });
        let yaml = serde_yaml::to_string(&meta).unwrap();
        let back: Metadata = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(meta, back);
        assert!(back.is_hidden_from_default_views());
    }
}
