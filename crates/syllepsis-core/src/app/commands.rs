//! The application command surface: framework-agnostic operations over a [`Book`].
//!
//! These are the functions the Tauri shell will expose as `#[tauri::command]` wrappers (and a
//! PWA worker can call directly). Keeping the logic here — not in the Tauri layer — means it
//! is unit-testable without a running app and shared across delivery targets (platform-infra.md
//! "share as much code as possible").

use chrono::Utc;

use crate::app::dto::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::dialect;
use crate::model::metadata::LockMode;
use crate::model::{Category, Note, ObjectType, PriorEdge};
use crate::sort::{self, RenderItem};
use crate::storage::{Book, NoteStore};

/// Render all sorted notes as the continuous book view.
pub fn book_view(book: &Book) -> CoreResult<Vec<RenderItem>> {
    let notes = book.store.read_all_notes()?;
    let categories = book.store.categories()?;
    Ok(sort::render(notes, categories))
}

/// Export the book view as a single linear markdown manuscript.
pub fn export_markdown(book: &Book) -> CoreResult<String> {
    Ok(sort::to_markdown(&book_view(book)?))
}

/// The unsorted queue: quick captures awaiting organization. Excludes hidden (archived/private)
/// and marked-for-deletion notes; newest first.
pub fn unsorted_notes(book: &Book) -> CoreResult<Vec<NoteDto>> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| !n.is_sorted() && n.metadata.is_visible_in_default_views())
        .collect();
    // ulid is time-ordered; descending gives newest-first.
    notes.sort_by(|a, b| b.id.ulid().cmp(a.id.ulid()));
    Ok(notes.iter().map(NoteDto::from_note).collect())
}

/// All categories defined in the book.
pub fn all_categories(book: &Book) -> CoreResult<Vec<Category>> {
    book.store.categories()
}

/// Every visible note (hidden / pending-deletion excluded), title-sorted. Backs views that
/// need the whole corpus at once — e.g. the graph view's nodes and edges.
pub fn list_notes(book: &Book) -> CoreResult<Vec<NoteDto>> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| n.metadata.is_visible_in_default_views())
        .collect();
    notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(notes.iter().map(NoteDto::from_note).collect())
}

/// Fetch a single note by id string.
pub fn get_note(book: &Book, id: &str) -> CoreResult<NoteDto> {
    let id = NoteId::parse(id)?;
    Ok(NoteDto::from_note(&book.store.read_note(&id)?))
}

/// Create a new unsorted note. If `inherit_from` is given, the new note copies that note's
/// categories (the "New Note inherits the selected note's categories" behavior, ui-views.md).
pub fn create_note(
    book: &Book,
    object_type: ObjectType,
    title: &str,
    inherit_from: Option<&str>,
) -> CoreResult<NoteDto> {
    let mut note = book.new_note(object_type, title)?;
    if let Some(source_id) = inherit_from {
        let source = book.store.read_note(&NoteId::parse(source_id)?)?;
        if !source.categories.is_empty() {
            note.categories = source.categories.clone();
            book.save_note(&note)?;
        }
    }
    Ok(NoteDto::from_note(&note))
}

/// Persist edits to a note. Bumps the updated timestamp and folds any inline `#tags` in the
/// body into the category set (object-types.md: inline categories merge into the loose array).
///
/// Two policies are enforced here against the *stored* note: a locked note's body is protected
/// from direct edits (privacy-security.md — body changes must go through unlock or a proposed
/// rewrite), and editing a knowledge-pack note marks it `locally_modified` so a later pack-version
/// re-import will not overwrite the user's change (core-concepts.md).
pub fn update_note(book: &Book, dto: NoteDto) -> CoreResult<NoteDto> {
    let mut note = dto.into_note(book.config.markdown.dialect_version.clone())?;
    if let Ok(stored) = book.store.read_note(&note.id) {
        if stored.metadata.lifecycle.lock != LockMode::None && stored.body != note.body {
            return Err(CoreError::Locked(format!(
                "'{}' is locked; unlock it or accept a proposed rewrite to change its body",
                note.title
            )));
        }
        if !note.metadata.packs.packs.is_empty() && content_changed(&stored, &note) {
            note.metadata.packs.locally_modified = true;
        }
    }
    note.metadata.dates.updated = Utc::now();
    merge_inline_categories(&mut note);
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Whether a user-meaningful field of a note changed (the parts a pack re-import would overwrite).
/// Lifecycle/authorship/date churn does not count as a "local modification".
fn content_changed(before: &Note, after: &Note) -> bool {
    before.title != after.title
        || before.summary != after.summary
        || before.body != after.body
        || before.categories != after.categories
}

/// Set (or clear) a note's sort position.
pub fn set_prior(book: &Book, id: &str, prior: Option<PriorEdge>) -> CoreResult<NoteDto> {
    let id = NoteId::parse(id)?;
    let mut note = book.store.read_note(&id)?;
    note.prior = prior;
    note.metadata.dates.updated = Utc::now();
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Fork a note into a new identity that records its lineage.
pub fn fork_note(book: &Book, id: &str) -> CoreResult<NoteDto> {
    let forked = book.fork_note(&NoteId::parse(id)?)?;
    Ok(NoteDto::from_note(&forked))
}

/// Permanently delete a note. (The "mark for deletion" delay and purge are Phase 6; the
/// `lifecycle.marked_for_deletion_at` field already exists to support it.)
pub fn delete_note(book: &Book, id: &str) -> CoreResult<()> {
    book.delete_note(&NoteId::parse(id)?)
}

/// Create or overwrite a category.
pub fn create_category(book: &Book, category: Category) -> CoreResult<()> {
    book.store.write_category(&category)
}

/// Fold inline `#tags` from the body into the note's category array (deduplicated).
fn merge_inline_categories(note: &mut Note) {
    for tag in dialect::extract_categories(&note.body) {
        if !note.categories.contains(&tag) {
            note.categories.push(tag);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    #[test]
    fn create_edit_and_render_a_book() {
        let (_dir, book) = book();
        create_category(&book, Category::new("intro")).unwrap();

        let mut a = create_note(&book, ObjectType::Note, "first", None).unwrap();
        a.body = "Opening line. #intro".into();
        a.prior = Some(PriorEdge::starts_category("intro"));
        a.sorted = true;
        let a = update_note(&book, a).unwrap();
        // Inline #intro folded into categories.
        assert!(a.categories.contains(&"intro".to_string()));

        let view = book_view(&book).unwrap();
        assert!(matches!(view[0], RenderItem::Heading { .. }));
        let md = export_markdown(&book).unwrap();
        assert!(md.contains("Opening line."));
    }

    #[test]
    fn new_note_inherits_categories() {
        let (_dir, book) = book();
        let mut source = create_note(&book, ObjectType::Note, "source", None).unwrap();
        source.categories = vec!["energy".into(), "design".into()];
        let source = update_note(&book, source).unwrap();

        let child = create_note(&book, ObjectType::Note, "child", Some(&source.id)).unwrap();
        assert_eq!(
            child.categories,
            vec!["energy".to_string(), "design".into()]
        );
    }

    #[test]
    fn locked_note_rejects_a_direct_body_edit() {
        let (_dir, book) = book();
        let note = create_note(&book, ObjectType::Note, "protected", None).unwrap();
        crate::app::lifecycle::set_note_lock(&book, &note.id, LockMode::UnlockDelay).unwrap();

        let mut edit = get_note(&book, &note.id).unwrap();
        edit.body = "a direct overwrite that should be refused".into();
        assert!(matches!(
            update_note(&book, edit).unwrap_err(),
            CoreError::Locked(_)
        ));

        // Unlocking first, then editing, is the supported path.
        crate::app::lifecycle::set_note_lock(&book, &note.id, LockMode::None).unwrap();
        let mut edit = get_note(&book, &note.id).unwrap();
        edit.body = "now editable".into();
        assert_eq!(update_note(&book, edit).unwrap().body, "now editable");
    }

    #[test]
    fn unsorted_queue_excludes_sorted_and_archived() {
        let (_dir, book) = book();
        let _unsorted = create_note(&book, ObjectType::Note, "capture", None).unwrap();

        let mut sorted = create_note(&book, ObjectType::Note, "placed", None).unwrap();
        sorted.prior = Some(PriorEdge::starts_category("c"));
        sorted.sorted = true;
        update_note(&book, sorted).unwrap();

        let queue = unsorted_notes(&book).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].title, "capture");
    }
}
