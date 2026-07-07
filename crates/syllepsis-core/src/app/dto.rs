//! Data-transfer objects for the application boundary (Tauri commands / future PWA worker).
//!
//! Why a separate type from [`Note`]: `Note`'s serde is tuned for *frontmatter* and skips the
//! `body` (the body lives below the frontmatter on disk). The API boundary must carry the
//! body, so [`NoteDto`] is the explicit over-the-wire shape. Keeping it distinct also means
//! the on-disk format and the API can evolve independently behind this seam.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::id::NoteId;
use crate::model::{
    AssetMetadata, CommentaryMetadata, EncryptionMeta, Metadata, Note, ObjectType, PriorEdge,
};

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
    /// The body the editor last loaded, sent back alongside an edit so `update_note` can tell a
    /// normal save from a stale one (the note changed on disk since the editor opened it) and
    /// fold the save through the CRDT layer instead of blindly overwriting. `None` for callers
    /// that don't track a baseline (older clients, plugin writes) — those keep the legacy
    /// overwrite behavior untouched.
    #[serde(default)]
    pub baseline_body: Option<String>,
    /// True when the note is PIN-locked (`encryption.is_some()` on the stored [`Note`]).
    #[serde(default)]
    pub pin_locked: bool,
    /// True when `summary`/`body` above are real plaintext rather than the locked placeholder.
    /// Only ever set by [`NoteDto::from_decrypted`] — never by [`NoteDto::from_note`], which is
    /// the fail-safe default every other core read path uses.
    #[serde(default)]
    pub unlocked: bool,
    /// Internal-only channel between the tauri boundary and [`into_note`](Self::into_note):
    /// when re-encrypting a locked note's edit via `pinlock::encrypt_for_save`, the resulting
    /// [`EncryptionMeta`] is stashed here before calling this crate's `update_note`. Never
    /// serialized in either direction — a DTO arriving from the wire can never carry (or forge)
    /// encryption metadata this way.
    #[serde(skip)]
    pub encryption: Option<EncryptionMeta>,
}

impl NoteDto {
    /// Project a stored note into its API shape. **Fail-safe at the core**: whenever the note is
    /// PIN-locked, `summary`/`body` are blanked here regardless of caller — core can never leak
    /// locked content through any DTO path. The only way a DTO carries real plaintext for a locked
    /// note is [`NoteDto::from_decrypted`], called explicitly at the tauri boundary once a session
    /// key is available.
    pub fn from_note(note: &Note) -> NoteDto {
        let pin_locked = note.is_pin_locked();
        NoteDto {
            id: note.id.to_string(),
            object_type: note.object_type,
            title: note.title.clone(),
            summary: if pin_locked {
                String::new()
            } else {
                note.summary.clone()
            },
            body: if pin_locked {
                String::new()
            } else {
                note.body.clone()
            },
            categories: note.categories.clone(),
            prior: note.prior.clone(),
            location: note.location.clone(),
            asset: note.asset.clone(),
            commentary: note.commentary.clone(),
            sorted: note.is_sorted(),
            metadata: note.metadata.clone(),
            baseline_body: None,
            pin_locked,
            unlocked: false,
            encryption: note.encryption.clone(),
        }
    }

    /// Like [`Self::from_note`] but with `summary`/`body` already decrypted by the caller (the
    /// tauri boundary's `present()` helper, once a session key is available). Core itself never
    /// calls this — it is the one sanctioned bypass of the blanking above.
    pub fn from_decrypted(note: &Note, summary: String, body: String) -> NoteDto {
        let mut dto = NoteDto::from_note(note);
        dto.summary = summary;
        dto.body = body;
        dto.unlocked = true;
        dto
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
            encryption: self.encryption,
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
