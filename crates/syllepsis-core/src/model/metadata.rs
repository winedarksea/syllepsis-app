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
/// managed by the app. Task dates distinguish planned and actual starts/ends:
/// `scheduled` is expected start, `started` is actual start, `due` is expected end, and
/// `completed` is actual end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateMetadata {
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<FlexDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<FlexDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<FlexDate>,
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
            started: None,
            due: None,
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
            started: None,
            due: None,
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
///
/// Privacy is split into three independent capabilities ([`Self::hidden`],
/// [`Self::exclude_from_search`], [`Self::exclude_from_publish`]) so a note can, say, stay out of
/// the public publish while remaining locally searchable. The legacy single `private` flag is a
/// convenience preset that turns all three on at once (see [`crate::app::lifecycle::set_note_private`]);
/// legacy notes carrying `private: true` are migrated by [`Self::normalize`] at the load boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Lifecycle {
    /// Not shown in the main UI / default views / exports.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Excluded from search + RAG retrieval.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclude_from_search: bool,
    /// Added to `.gitignore` and excluded from the static-site publish.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclude_from_publish: bool,
    /// Deserialize-only capture of the legacy single `private` flag. Never written back
    /// (`skip_serializing`); [`Self::normalize`] fans it out to the three flags above at load.
    #[serde(default, skip_serializing, rename = "private")]
    legacy_private: bool,
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

impl Lifecycle {
    /// Migrate a legacy `private: true` note to the three independent capability flags. Called at
    /// the single storage read boundary ([`crate::markdown::frontmatter::parse_note`]) so every
    /// downstream feature sees expanded flags and the legacy key is never written back. Must **not**
    /// touch any cloud-sync flag — legacy private notes keep syncing exactly as before.
    pub fn normalize(&mut self) {
        if self.legacy_private {
            self.hidden = true;
            self.exclude_from_search = true;
            self.exclude_from_publish = true;
            self.legacy_private = false;
        }
    }
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

/// Shared note/task status vocabulary. Todo checkbox markers are parsed into this same enum so
/// whole-note status and line-level task status cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteStatus {
    Open,
    Active,
    NeedsClarification,
    Deferred,
    Cancelled,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteVisibility {
    #[default]
    Active,
    Archived,
    Trash,
}

impl NoteStatus {
    pub fn checkbox_marker(self) -> &'static str {
        match self {
            NoteStatus::Open => " ",
            NoteStatus::Active => "/",
            NoteStatus::NeedsClarification => "?",
            NoteStatus::Deferred => ">",
            NoteStatus::Cancelled => "-",
            NoteStatus::Done => "x",
        }
    }
}

pub fn status_from_checkbox_marker(marker: &str) -> Option<NoteStatus> {
    match marker {
        " " => Some(NoteStatus::Open),
        "/" => Some(NoteStatus::Active),
        "?" => Some(NoteStatus::NeedsClarification),
        ">" => Some(NoteStatus::Deferred),
        "-" => Some(NoteStatus::Cancelled),
        "x" | "X" => Some(NoteStatus::Done),
        _ => None,
    }
}

pub fn checkbox_marker_for_status(status: NoteStatus) -> &'static str {
    status.checkbox_marker()
}

/// The complete metadata bundle embedded in a [`super::Note`]. Default sections are skipped
/// on serialize so a plain note's frontmatter stays minimal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<NoteStatus>,
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

    /// Whether the note is currently hidden from default views (explicitly hidden or archived).
    pub fn is_hidden_from_default_views(&self) -> bool {
        self.lifecycle.hidden || self.lifecycle.archived
    }

    /// Whether the note should appear in default views: not hidden (explicitly hidden or archived)
    /// and not pending deletion. This is the single predicate the read paths (unsorted queue, note
    /// list, overlays) share so "what the user sees by default" has one definition
    /// (privacy-security.md).
    pub fn is_visible_in_default_views(&self) -> bool {
        !self.is_hidden_from_default_views() && self.lifecycle.marked_for_deletion_at.is_none()
    }

    /// Whether the note participates in search + RAG retrieval. Independent from visibility now that
    /// search-exclusion is its own capability: a note can be visible in default views but excluded
    /// from search, or vice versa. Still requires the note be visible by default (a hidden/archived/
    /// pending-deletion note never surfaces in default search results).
    pub fn is_searchable(&self) -> bool {
        self.is_visible_in_default_views() && !self.lifecycle.exclude_from_search
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_note_serializes_dates_and_classification() {
        let meta = Metadata::now();
        let yaml = serde_yaml::to_string(&meta).unwrap();
        // classification is always serialized (ensures it's never undefined on the API boundary);
        // other optional sub-sections are still skipped when default.
        assert!(yaml.contains("dates:"));
        assert!(yaml.contains("classification:"));
        assert!(!yaml.contains("kanban:"));
        assert!(!yaml.contains("lifecycle:"));
    }

    #[test]
    fn round_trips_with_lifecycle_and_fork() {
        let mut meta = Metadata::now();
        meta.lifecycle.hidden = true;
        meta.lifecycle.exclude_from_search = true;
        meta.lifecycle.exclude_from_publish = true;
        meta.lifecycle.lock = LockMode::UnlockDelay;
        meta.fork = Some(ForkInfo {
            forked_from: NoteId::generate("note", "parent"),
            forked_at: Utc::now(),
        });
        let yaml = serde_yaml::to_string(&meta).unwrap();
        let back: Metadata = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(meta, back);
        assert!(back.is_hidden_from_default_views());
        assert!(!back.is_searchable());
    }

    #[test]
    fn legacy_private_flag_migrates_to_three_capabilities() {
        // A legacy note frontmatter carrying the single `private: true` flag.
        let mut life: Lifecycle = serde_yaml::from_str("private: true").unwrap();
        // Before normalize the legacy flag is captured but the new flags are untouched.
        assert!(!life.hidden && !life.exclude_from_search && !life.exclude_from_publish);

        life.normalize();
        assert!(life.hidden);
        assert!(life.exclude_from_search);
        assert!(life.exclude_from_publish);

        // Re-serialization emits the three new keys and never the legacy `private` key.
        let yaml = serde_yaml::to_string(&life).unwrap();
        assert!(yaml.contains("hidden: true"));
        assert!(yaml.contains("exclude_from_search: true"));
        assert!(yaml.contains("exclude_from_publish: true"));
        assert!(!yaml.contains("private"));

        // Idempotent: re-parsing the new form and normalizing is a no-op.
        let mut reparsed: Lifecycle = serde_yaml::from_str(&yaml).unwrap();
        reparsed.normalize();
        assert_eq!(life, reparsed);
    }

    #[test]
    fn status_is_optional_and_round_trips_when_present() {
        let mut meta = Metadata::now();
        let yaml = serde_yaml::to_string(&meta).unwrap();
        assert!(!yaml.contains("status:"));

        for status in [
            NoteStatus::Open,
            NoteStatus::Active,
            NoteStatus::NeedsClarification,
            NoteStatus::Deferred,
            NoteStatus::Cancelled,
            NoteStatus::Done,
        ] {
            meta.status = Some(status);
            let yaml = serde_yaml::to_string(&meta).unwrap();
            let back: Metadata = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(back.status, Some(status));
        }
    }

    #[test]
    fn todo_checkbox_markers_share_note_status_enum() {
        let cases = [
            (" ", NoteStatus::Open),
            ("/", NoteStatus::Active),
            ("?", NoteStatus::NeedsClarification),
            (">", NoteStatus::Deferred),
            ("-", NoteStatus::Cancelled),
            ("x", NoteStatus::Done),
        ];
        for (marker, status) in cases {
            assert_eq!(status_from_checkbox_marker(marker), Some(status));
            assert_eq!(checkbox_marker_for_status(status), marker);
        }
        assert_eq!(status_from_checkbox_marker("!"), None);
    }
}
