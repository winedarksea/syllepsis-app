//! The note-write host API: the small, stable surface a plugin uses to *change notes*.
//!
//! Per the design, a plugin never touches Loro or the filesystem directly. It hands the host a
//! high-level intent — "create this note", "replace this note's body" — and the host applies it
//! through the same [`crate::app::commands`] paths the UI uses, so the change flows into the CRDT
//! sidecar at sync time exactly like a normal edit. This milestone implements the two operations
//! the contract needs; fine-grained text-diff / JSON-op translation into Loro is deferred until a
//! plugin actually needs it.
//!
//! These are plain functions over a [`Book`] so they are unit-testable without a WASM runtime; the
//! `extism` host-function wiring in [`super::host`] is a thin shim that decodes the call and
//! forwards here.

use serde::{Deserialize, Serialize};

use crate::app::commands;
use crate::app::dto::NoteDto;
use crate::error::CoreResult;
use crate::model::ObjectType;
use crate::storage::Book;

/// A plugin's request to create a new note (the JSON payload of the `create_note` host function).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateNoteInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Object type id prefix (e.g. `"note"`, `"quote"`); defaults to a plain note.
    #[serde(default)]
    pub object_type: Option<String>,
}

/// Create a note from a plugin request and return its id. Routes through
/// [`commands::create_note`] / [`commands::update_note`] so category declaration, inline-tag
/// folding, and timestamps behave identically to a UI-created note.
pub fn create_note(book: &Book, input: &CreateNoteInput) -> CoreResult<String> {
    let object_type = input
        .object_type
        .as_deref()
        .and_then(ObjectType::from_id_prefix)
        .unwrap_or(ObjectType::Note);
    let created = commands::create_note(book, object_type, &input.title, None)?;
    let updated = commands::update_note(
        book,
        NoteDto {
            body: input.body.clone(),
            summary: input.summary.clone(),
            categories: input.categories.clone(),
            ..created
        },
    )?;
    Ok(updated.id)
}

/// Replace a note's body wholesale. Routes through [`commands::update_note`], so a locked note is
/// still protected and the change is picked up by sync.
pub fn replace_body(book: &Book, note_id: &str, body: &str) -> CoreResult<()> {
    let current = commands::get_note(book, note_id)?;
    commands::update_note(
        book,
        NoteDto {
            body: body.to_string(),
            ..current
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempdir().unwrap();
        let book = Book::create(dir.path().join("book"), "Book").unwrap();
        (dir, book)
    }

    #[test]
    fn create_note_persists_body_and_categories() {
        let (_dir, book) = book();
        let id = create_note(
            &book,
            &CreateNoteInput {
                title: "From plugin".to_string(),
                body: "Extracted text.".to_string(),
                categories: vec!["imported".to_string()],
                ..CreateNoteInput::default()
            },
        )
        .unwrap();
        let stored = commands::get_note(&book, &id).unwrap();
        assert_eq!(stored.title, "From plugin");
        assert_eq!(stored.body, "Extracted text.");
        assert!(stored.categories.contains(&"imported".to_string()));
    }

    #[test]
    fn replace_body_overwrites_existing_note() {
        let (_dir, book) = book();
        let id = create_note(
            &book,
            &CreateNoteInput {
                title: "Note".to_string(),
                body: "before".to_string(),
                ..CreateNoteInput::default()
            },
        )
        .unwrap();
        replace_body(&book, &id, "after").unwrap();
        assert_eq!(commands::get_note(&book, &id).unwrap().body, "after");
    }
}
