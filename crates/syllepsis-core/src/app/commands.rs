//! The application command surface: framework-agnostic operations over a [`Book`].
//!
//! These are the functions the Tauri shell will expose as `#[tauri::command]` wrappers (and a
//! PWA worker can call directly). Keeping the logic here — not in the Tauri layer — means it
//! is unit-testable without a running app and shared across delivery targets (platform-infra.md
//! "share as much code as possible").

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::app::dto::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::dialect;
use crate::model::metadata::LockMode;
use crate::model::{Category, Note, NoteVisibility, ObjectType, PriorEdge};
use crate::sort::{self, RenderItem};
use crate::storage::{layout, Book, NoteStore};

/// Aggregate statistics about a book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookStats {
    pub total_notes: usize,
    pub sorted_notes: usize,
    pub unsorted_notes: usize,
    pub private_notes: usize,
    pub archived_notes: usize,
    pub starred_notes: usize,
    pub notes_by_type: HashMap<String, usize>,
    pub notes_by_category: HashMap<String, usize>,
    pub total_categories: usize,
    pub notes_with_location: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateNoteOptions {
    pub vanishing: bool,
    pub vanish_days: Option<u32>,
}

pub fn note_matches_visibility(note: &Note, visibility: NoteVisibility) -> bool {
    match visibility {
        NoteVisibility::Active => note.metadata.is_visible_in_default_views(),
        NoteVisibility::Archived => {
            note.metadata.lifecycle.archived
                && !note.metadata.lifecycle.private
                && note.metadata.lifecycle.marked_for_deletion_at.is_none()
        }
        NoteVisibility::Trash => {
            !note.metadata.lifecycle.private
                && note.metadata.lifecycle.marked_for_deletion_at.is_some()
        }
    }
}

/// Render all sorted notes as the continuous book view.
pub fn book_view(book: &Book) -> CoreResult<Vec<RenderItem>> {
    let notes = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| note_matches_visibility(n, NoteVisibility::Active))
        .collect();
    let categories = book.store.categories()?;
    Ok(sort::render(notes, categories))
}

/// Export the book view as a single linear markdown manuscript.
pub fn export_markdown(book: &Book) -> CoreResult<String> {
    Ok(sort::to_markdown(&book_view(book)?))
}

/// Export the book view as an HTML document, routing plugin-claimed code blocks through
/// `render_code_block`. Pass `&|_, _| None` for a plain export with no plugin rendering.
pub fn export_html(
    book: &Book,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> CoreResult<String> {
    let markdown = export_markdown(book)?;
    let cleaned = dialect::strip_comments(&markdown);
    Ok(crate::publish::build_export_html(
        &book.metadata.name,
        &cleaned,
        render_code_block,
    ))
}

/// Aggregate statistics about a book.
pub fn book_stats(book: &Book) -> CoreResult<BookStats> {
    let all_notes = book.store.read_all_notes()?;
    let categories = book.store.categories()?;

    let mut stats = BookStats {
        total_notes: 0,
        sorted_notes: 0,
        unsorted_notes: 0,
        private_notes: 0,
        archived_notes: 0,
        starred_notes: 0,
        notes_by_type: HashMap::new(),
        notes_by_category: HashMap::new(),
        total_categories: categories.len(),
        notes_with_location: 0,
    };

    for note in &all_notes {
        if note.metadata.lifecycle.marked_for_deletion_at.is_some() {
            continue;
        }
        stats.total_notes += 1;
        if note.is_sorted() {
            stats.sorted_notes += 1;
        } else {
            stats.unsorted_notes += 1;
        }
        if note.metadata.lifecycle.private {
            stats.private_notes += 1;
        }
        if note.metadata.lifecycle.archived {
            stats.archived_notes += 1;
        }
        if note.metadata.classification.starred {
            stats.starred_notes += 1;
        }
        if note.location.is_some() {
            stats.notes_with_location += 1;
        }
        let type_key = note.object_type.id_prefix().to_string();
        *stats.notes_by_type.entry(type_key).or_insert(0) += 1;
        for cat in &note.categories {
            *stats.notes_by_category.entry(cat.clone()).or_insert(0) += 1;
        }
    }
    Ok(stats)
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
    list_notes_with_visibility(book, NoteVisibility::Active)
}

pub fn list_notes_with_visibility(
    book: &Book,
    visibility: NoteVisibility,
) -> CoreResult<Vec<NoteDto>> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| note_matches_visibility(n, visibility))
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
    create_note_with_options(
        book,
        object_type,
        title,
        inherit_from,
        CreateNoteOptions::default(),
    )
}

pub fn create_note_with_options(
    book: &Book,
    object_type: ObjectType,
    title: &str,
    inherit_from: Option<&str>,
    options: CreateNoteOptions,
) -> CoreResult<NoteDto> {
    let mut note = book.new_note(object_type, title)?;
    if options.vanishing {
        let days = options
            .vanish_days
            .unwrap_or(book.config.cleanup.default_vanish_days);
        note.metadata.lifecycle.vanish_at = Some(Utc::now() + Duration::days(days as i64));
        book.save_note(&note)?;
    }
    if let Some(source_id) = inherit_from {
        let source = book.store.read_note(&NoteId::parse(source_id)?)?;
        if !source.categories.is_empty() {
            note.categories = source.categories.clone();
            book.save_note(&note)?;
        }
    }
    if object_type == ObjectType::Table {
        let empty: Vec<Vec<String>> = vec![vec![String::new(); 3]; 5];
        let csv_path = layout::table_companion_csv_path(&book.root, &note.id);
        std::fs::write(csv_path, encode_csv(&empty))?;
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
    if matches!(note.object_type, ObjectType::Picture | ObjectType::Drawing)
        && note.metadata.lifecycle.archived
    {
        return Err(CoreError::InvalidBook(
            "pictures and drawings cannot be archived; delete them instead".to_string(),
        ));
    }
    // Refresh the cosmetic slug so the filename tracks the current title.
    note.id = note.id.with_regenerated_slug(&note.title);
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
        // Remove categories that existed only because of a body #tag that has since been deleted.
        let old_body_cats = dialect::extract_categories(&stored.body);
        let new_body_cats = dialect::extract_categories(&note.body);
        let dropped: Vec<String> = old_body_cats
            .into_iter()
            .filter(|c| !new_body_cats.contains(c))
            .collect();
        note.categories.retain(|c| !dropped.contains(c));
    }
    note.metadata.dates.updated = Utc::now();
    merge_inline_categories(&mut note);
    ensure_categories_declared(book, &note.categories)?;
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Declare a category file for any category referenced by a note that does not yet have one.
/// Without this, a `#tag` added to a note only lives in the note's frontmatter and never appears
/// in the sidebar (which lists declared categories). Existing category files are left untouched so
/// user customizations (icon, long name, privacy) survive.
fn ensure_categories_declared(book: &Book, categories: &[String]) -> CoreResult<()> {
    for name in categories {
        if book.store.read_category(name).is_err() {
            book.store.write_category(&Category::new(name.clone()))?;
        }
    }
    Ok(())
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

/// Validate and import an image into the book's tracked `assets/` directory, returning the
/// book-relative path for an inline Markdown image reference.
pub fn import_asset(book: &Book, source_path: &str) -> CoreResult<String> {
    Ok(crate::app::image_assets::import_tracked_asset(book, source_path)?.relative_path)
}

/// Read the CSV companion file for a Table note. Returns an empty 5×3 grid if absent.
pub fn read_table_data(book: &Book, id: &str) -> CoreResult<Vec<Vec<String>>> {
    let id = NoteId::parse(id)?;
    let path = layout::table_companion_csv_path(&book.root, &id);
    if !path.exists() {
        return Ok(vec![vec![String::new(); 3]; 5]);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(decode_csv(&text))
}

/// Write the CSV companion file for a Table note.
pub fn save_table_data(book: &Book, id: &str, rows: Vec<Vec<String>>) -> CoreResult<()> {
    let id = NoteId::parse(id)?;
    let path = layout::table_companion_csv_path(&book.root, &id);
    std::fs::write(path, encode_csv(&rows))?;
    Ok(())
}

/// Encode a 2-D string grid as RFC-4180 CSV.
fn encode_csv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    if cell.contains(',') || cell.contains('"') || cell.contains('\n') {
                        format!("\"{}\"", cell.replace('"', "\"\""))
                    } else {
                        cell.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decode RFC-4180 CSV into a 2-D string grid.
fn decode_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                row.push(cell.clone());
                cell.clear();
            }
            '\n' if !in_quotes => {
                row.push(cell.clone());
                cell.clear();
                rows.push(row.clone());
                row.clear();
            }
            '\r' => {}
            c => cell.push(c),
        }
    }
    row.push(cell);
    if !row.iter().all(|c| c.is_empty()) || !rows.is_empty() {
        rows.push(row);
    }
    rows
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
    fn editing_a_note_declares_its_inline_categories() {
        let (_dir, book) = book();
        // No categories declared yet.
        assert!(all_categories(&book).unwrap().is_empty());

        let mut note = create_note(&book, ObjectType::Note, "first", None).unwrap();
        note.body = "An idea worth keeping. #research".into();
        update_note(&book, note).unwrap();

        // The inline #research tag should now be a declared category visible in the sidebar.
        let declared: Vec<String> = all_categories(&book)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(declared.contains(&"research".to_string()));
    }

    #[test]
    fn auto_declaring_categories_preserves_existing_customizations() {
        let (_dir, book) = book();
        let mut custom = Category::new("research");
        custom.long_name = "Deep Research".into();
        custom.icon = Some("🔬".into());
        create_category(&book, custom).unwrap();

        let mut note = create_note(&book, ObjectType::Note, "first", None).unwrap();
        note.body = "More. #research".into();
        update_note(&book, note).unwrap();

        let stored = book.store.read_category("research").unwrap();
        assert_eq!(stored.long_name, "Deep Research");
        assert_eq!(stored.icon.as_deref(), Some("🔬"));
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
    fn create_note_options_can_make_note_vanish() {
        let (_dir, book) = book();
        let note = create_note_with_options(
            &book,
            ObjectType::Note,
            "temporary",
            None,
            CreateNoteOptions {
                vanishing: true,
                vanish_days: Some(7),
            },
        )
        .unwrap();
        assert!(note.metadata.lifecycle.vanish_at.is_some());
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
