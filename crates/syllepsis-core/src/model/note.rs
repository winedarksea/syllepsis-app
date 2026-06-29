//! The central note object. In memory a note is its frontmatter fields plus a `body`
//! (the markdown after the frontmatter). On disk those are one file: the markdown layer
//! ([`crate::markdown`]) serializes everything here as YAML frontmatter and writes `body`
//! beneath it. `body` is therefore `#[serde(skip)]` — it is never part of the frontmatter.
//!
//! Every text object carries the **summary / full-description duality**: `summary` is the
//! flashcard-front/chapter-blurb view and `body` is the full text.

use crate::config::SummaryConfig;
use crate::id::NoteId;
use crate::model::metadata::Metadata;
use crate::model::object_type::ObjectType;
use crate::model::prior::PriorEdge;
use crate::model::CommentaryMetadata;
use serde::{Deserialize, Serialize};

/// Stable reference and display metadata for a Picture or Drawing payload.
///
/// The imported file remains untouched. This small record is canonical and travels with the
/// Markdown note while the UUID sidecar lets the file be renamed or moved independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub uuid: String,
    pub media_type: String,
    pub intrinsic_dimensions: (u32, u32),
    pub original_filename: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    #[serde(rename = "type")]
    pub object_type: ObjectType,
    /// Dialect tag (e.g. `syllepsis_001`) so files read outside the app trace their origin.
    pub markdown_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The short "summary" view (question, for QA notes).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// The full markdown body (answer, for QA notes). Lives below the frontmatter on disk.
    #[serde(skip)]
    pub body: String,
    /// The loose category array (no-whitespace names). Inline `#tags` in the body are merged
    /// into this set by the markdown layer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// Sort position: what this note follows. `None` means the note is **unsorted**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<PriorEdge>,
    /// Optional note-level location token pinning the whole note to a coordinate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Present for first-class Picture and Drawing objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetMetadata>,
    /// Present only for commentary child objects stored under `_commentary/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commentary: Option<CommentaryMetadata>,
    pub metadata: Metadata,
}

/// Result of the summary/full-description alignment check (object-types.md).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryWarning {
    pub summary_chars: usize,
    pub body_chars: usize,
    /// summary length as a fraction of body length.
    pub ratio: f32,
}

impl Note {
    /// Create a fresh, **unsorted** note of the given type. The id's ulid is minted now.
    pub fn new(
        object_type: ObjectType,
        title: impl Into<String>,
        dialect_version: impl Into<String>,
    ) -> Note {
        let title = title.into();
        let id = NoteId::generate(object_type.id_prefix(), &title);
        Note {
            id,
            object_type,
            markdown_version: dialect_version.into(),
            title,
            summary: String::new(),
            body: String::new(),
            categories: Vec::new(),
            prior: None,
            location: None,
            asset: None,
            commentary: None,
            metadata: Metadata::now(),
        }
    }

    /// A note has a place in the narrative iff it has a prior.
    pub fn is_sorted(&self) -> bool {
        self.prior.is_some()
    }

    /// Rename the title, regenerating the cosmetic slug while preserving the canonical ulid.
    pub fn retitle(&mut self, new_title: impl Into<String>) {
        let new_title = new_title.into();
        self.id = self.id.with_regenerated_slug(&new_title);
        self.title = new_title;
    }

    /// Check whether the summary is too long relative to the body. Returns `Some` when the
    /// summary exceeds **both** the absolute char cap and the fraction-of-body cap is
    /// surpassed — i.e. it warns past the larger of the two limits (per object-types.md).
    pub fn summary_warning(&self, cfg: &SummaryConfig) -> Option<SummaryWarning> {
        let summary_chars = self.summary.chars().count();
        let body_chars = self.body.chars().count();
        if summary_chars == 0 {
            return None;
        }
        let fraction_limit = (body_chars as f32 * cfg.max_fraction_of_body).round() as usize;
        let limit = cfg.max_chars.max(fraction_limit);
        let ratio = if body_chars == 0 {
            f32::INFINITY
        } else {
            summary_chars as f32 / body_chars as f32
        };
        if summary_chars > limit {
            Some(SummaryWarning {
                summary_chars,
                body_chars,
                ratio,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_note_is_unsorted_with_minted_id() {
        let note = Note::new(
            ObjectType::Note,
            "Energy efficiency basics",
            "syllepsis_001",
        );
        assert!(!note.is_sorted());
        assert_eq!(note.object_type, ObjectType::Note);
        assert_eq!(note.id.type_prefix(), "note");
        assert_eq!(note.markdown_version, "syllepsis_001");
    }

    #[test]
    fn retitle_keeps_identity() {
        let mut note = Note::new(ObjectType::Note, "old", "syllepsis_001");
        let original_ulid = note.id.ulid().to_string();
        note.retitle("a brand new and much longer title");
        assert_eq!(note.id.ulid(), original_ulid);
        assert!(note.id.slug().contains("brand"));
    }

    #[test]
    fn summary_warning_respects_larger_limit() {
        let cfg = SummaryConfig::default(); // 250 chars or 10% of body
        let mut note = Note::new(ObjectType::Note, "t", "syllepsis_001");
        // Short summary, no body → under the 250 cap → no warning.
        note.summary = "a".repeat(100);
        assert!(note.summary_warning(&cfg).is_none());
        // Over the 250 cap with a small body → warning.
        note.summary = "a".repeat(300);
        note.body = "b".repeat(100);
        let w = note
            .summary_warning(&cfg)
            .expect("should warn over 250 chars");
        assert_eq!(w.summary_chars, 300);
        // Big body lifts the fraction limit above the summary length → no warning.
        note.body = "b".repeat(4000); // 10% = 400 > 300
        assert!(note.summary_warning(&cfg).is_none());
    }
}
