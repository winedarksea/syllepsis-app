//! Data-transfer objects for the application boundary (Tauri commands / future PWA worker).
//!
//! Why a separate type from [`Note`]: `Note`'s serde is tuned for *frontmatter* and skips the
//! `body` (the body lives below the frontmatter on disk). The API boundary must carry the
//! body, so [`NoteDto`] is the explicit over-the-wire shape. Keeping it distinct also means
//! the on-disk format and the API can evolve independently behind this seam.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::id::NoteId;
use crate::model::{AssetMetadata, CommentaryMetadata, Metadata, Note, ObjectType, PriorEdge};

/// A note as sent to / received from the UI. Unlike [`Note`], this includes the body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteDto {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: ObjectType,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub categories: Vec<String>,
    #[serde(default)]
    pub prior: Option<PriorEdge>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub asset: Option<AssetMetadata>,
    #[serde(default)]
    pub commentary: Option<CommentaryMetadata>,
    /// Convenience flag for the UI (mirrors `prior.is_some()`).
    pub sorted: bool,
    pub metadata: Metadata,
}

impl NoteDto {
    /// Project a stored note into its API shape.
    pub fn from_note(note: &Note) -> NoteDto {
        NoteDto {
            id: note.id.to_string(),
            object_type: note.object_type,
            title: note.title.clone(),
            summary: note.summary.clone(),
            body: note.body.clone(),
            categories: note.categories.clone(),
            prior: note.prior.clone(),
            location: note.location.clone(),
            asset: note.asset.clone(),
            commentary: note.commentary.clone(),
            sorted: note.is_sorted(),
            metadata: note.metadata.clone(),
        }
    }

    /// Rebuild a stored note from an incoming DTO, stamping the book's dialect version.
    pub fn into_note(self, dialect_version: impl Into<String>) -> CoreResult<Note> {
        Ok(Note {
            id: NoteId::parse(&self.id)?,
            object_type: self.object_type,
            markdown_version: dialect_version.into(),
            title: self.title,
            summary: self.summary,
            body: self.body,
            categories: self.categories,
            prior: self.prior,
            location: self.location,
            asset: self.asset,
            commentary: self.commentary,
            metadata: self.metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_round_trips_through_a_note_with_body() {
        let mut note = Note::new(ObjectType::Note, "t", "syllepsis_001");
        note.body = "body that frontmatter serde would drop".into();
        let dto = NoteDto::from_note(&note);
        assert_eq!(dto.body, note.body);

        // The DTO serializes *with* the body (unlike the frontmatter form).
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("body that frontmatter serde would drop"));

        let back = dto.into_note("syllepsis_001").unwrap();
        assert_eq!(back.id, note.id);
        assert_eq!(back.body, note.body);
    }
}
