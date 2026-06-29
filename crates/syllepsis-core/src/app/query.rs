//! Lightweight query helpers shared by the REST/MCP API and any future callers.
//! These mirror common UI views (recent, core, by-category) without requiring a search engine.

use crate::app::commands::{list_notes_with_visibility, note_matches_visibility};
use crate::app::dto::NoteDto;
use crate::error::CoreResult;
use crate::model::{classification::Priority, NoteVisibility};
use crate::storage::{Book, NoteStore};

/// `n` most recently updated visible notes, newest first.
pub fn recent_notes(book: &Book, n: usize) -> CoreResult<Vec<NoteDto>> {
    let mut notes = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| note_matches_visibility(note, NoteVisibility::Active))
        .collect::<Vec<_>>();
    notes.sort_by(|a, b| b.metadata.dates.updated.cmp(&a.metadata.dates.updated));
    notes.truncate(n);
    Ok(notes.iter().map(NoteDto::from_note).collect())
}

/// All visible notes with `classification.priority == Core`.
pub fn core_notes(book: &Book) -> CoreResult<Vec<NoteDto>> {
    let notes = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| {
            note_matches_visibility(note, NoteVisibility::Active)
                && note.metadata.classification.priority == Priority::Core
        })
        .collect::<Vec<_>>();
    Ok(notes.iter().map(NoteDto::from_note).collect())
}

/// All visible notes in `category` (case-sensitive match against the note's category list).
pub fn notes_by_category(book: &Book, category: &str) -> CoreResult<Vec<NoteDto>> {
    let notes = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| {
            note_matches_visibility(note, NoteVisibility::Active)
                && note.categories.iter().any(|c| c == category)
        })
        .collect::<Vec<_>>();
    Ok(notes.iter().map(NoteDto::from_note).collect())
}

/// Thin wrapper around `list_notes_with_visibility` for consumers that want all active notes
/// title-sorted (e.g. the list endpoint).
pub fn all_active_notes(book: &Book) -> CoreResult<Vec<NoteDto>> {
    list_notes_with_visibility(book, NoteVisibility::Active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, update_note};
    use crate::model::{classification::Priority, ObjectType};
    use crate::storage::Book;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    #[test]
    fn recent_notes_returns_newest_first() {
        let (_dir, book) = book();
        let _a = create_note(&book, ObjectType::Note, "alpha", None).unwrap();
        let _b = create_note(&book, ObjectType::Note, "beta", None).unwrap();
        let recent = recent_notes(&book, 5).unwrap();
        // beta was created last, so it should appear first
        assert_eq!(recent[0].title, "beta");
        assert_eq!(recent[1].title, "alpha");
    }

    #[test]
    fn recent_notes_truncates_to_n() {
        let (_dir, book) = book();
        for i in 0..10 {
            create_note(&book, ObjectType::Note, &format!("note{i}"), None).unwrap();
        }
        let recent = recent_notes(&book, 3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn core_notes_filters_by_priority() {
        let (_dir, book) = book();
        let mut core_note = create_note(&book, ObjectType::Note, "core", None).unwrap();
        core_note.metadata.classification.priority = Priority::Core;
        update_note(&book, core_note).unwrap();
        let _standard = create_note(&book, ObjectType::Note, "standard", None).unwrap();

        let cores = core_notes(&book).unwrap();
        assert_eq!(cores.len(), 1);
        assert_eq!(cores[0].title, "core");
    }

    #[test]
    fn notes_by_category_matches_category() {
        let (_dir, book) = book();
        let mut tagged = create_note(&book, ObjectType::Note, "tagged", None).unwrap();
        tagged.categories = vec!["science".into()];
        update_note(&book, tagged).unwrap();
        let _other = create_note(&book, ObjectType::Note, "other", None).unwrap();

        let results = notes_by_category(&book, "science").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "tagged");

        let none = notes_by_category(&book, "nonexistent").unwrap();
        assert!(none.is_empty());
    }
}
