//! The application command surface: framework-agnostic operations over a [`Book`].
//!
//! These are the functions the Tauri shell will expose as `#[tauri::command]` wrappers (and a
//! PWA worker can call directly). Keeping the logic here — not in the Tauri layer — means it
//! is unit-testable without a running app and shared across delivery targets (platform-infra.md
//! "share as much code as possible").

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::app::dto::NoteDto;
use crate::embeddings::repository::configured_model_fingerprint;
use crate::embeddings::{read_sidecar, StoredEmbedding};
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::dialect;
use crate::model::metadata::{LockMode, Metadata};
use crate::model::{Category, Note, NoteVisibility, ObjectType, PriorEdge};
use crate::publish;
use crate::sort::{self, RenderItem};
use crate::storage::{layout, Book, NoteStore};
use pulldown_cmark::Options;
use regex::Regex;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteNeighborSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteNeighbors {
    pub previous: Option<NoteNeighborSummary>,
    pub next: Option<NoteNeighborSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteTokenCountMethod {
    EmbeddingTokenizer,
    SharedTokenizer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTokenCount {
    pub count: usize,
    pub method: NoteTokenCountMethod,
    pub warning: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteEmbeddingDetails {
    pub status: String,
    pub generated_at_unix_ms: Option<i64>,
    pub model_id: Option<String>,
    pub dimensions: Option<usize>,
    pub summary_vector: Option<Vec<f32>>,
    pub full_note_vector: Option<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeNotesRequest {
    pub target_note_id: String,
    pub source_note_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitNoteRequest {
    pub note_id: String,
    pub split_at: usize,
    pub second_title: Option<String>,
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

pub fn render_note_markdown(
    book: &Book,
    note_id: Option<&str>,
    markdown: Option<&str>,
    render_code_block: &dyn Fn(&str, &str) -> Option<String>,
) -> CoreResult<String> {
    let body = match (note_id, markdown) {
        (_, Some(markdown)) => markdown.to_string(),
        (Some(note_id), None) => get_note(book, note_id)?.body,
        (None, None) => String::new(),
    };
    let cleaned = dialect::strip_comments(&body);
    let with_clozes = renderable_cloze_markup(&cleaned);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let mut html = String::new();
    publish::push_html_with_plugins(&mut html, &with_clozes, options, render_code_block);
    Ok(html)
}

static CLOZE_MARKUP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|\|(.+?)\|\|").unwrap());

fn renderable_cloze_markup(markdown: &str) -> String {
    CLOZE_MARKUP_RE
        .replace_all(markdown, |captures: &regex::Captures<'_>| {
            let cloze = parse_cloze_markup_inner(&captures[1]);
            let label = cloze
                .hint
                .filter(|hint| !hint.trim().is_empty())
                .unwrap_or_else(|| "show".to_string());
            format!(
                "<button type=\"button\" class=\"syl-cloze\" data-hidden=\"{}\">{}</button>",
                escape_html_attr(&cloze.hidden),
                escape_html_text(&label)
            )
        })
        .into_owned()
}

struct ClozeMarkup {
    hidden: String,
    hint: Option<String>,
}

fn parse_cloze_markup_inner(inner: &str) -> ClozeMarkup {
    let remainder = match inner.split_once("::") {
        Some((group, rest))
            if !group.is_empty()
                && group
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_') =>
        {
            rest
        }
        _ => inner,
    };
    match remainder.split_once('|') {
        Some((hidden, hint)) => ClozeMarkup {
            hidden: hidden.to_string(),
            hint: Some(hint.to_string()),
        },
        None => ClozeMarkup {
            hidden: remainder.to_string(),
            hint: None,
        },
    }
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html_text(value).replace('"', "&quot;")
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

pub fn note_neighbors(book: &Book, id: &str) -> CoreResult<NoteNeighbors> {
    let target = NoteId::parse(id)?;
    let items = book_view(book)?;
    let ordered_ids: Vec<NoteId> = items
        .into_iter()
        .filter_map(|item| match item {
            RenderItem::Note(note) => Some(note.id),
            RenderItem::Heading { .. } => None,
        })
        .collect();
    let Some(index) = ordered_ids
        .iter()
        .position(|candidate| candidate == &target)
    else {
        return Ok(NoteNeighbors {
            previous: None,
            next: None,
        });
    };
    let previous = index
        .checked_sub(1)
        .and_then(|previous_index| neighbor_summary(book, &ordered_ids[previous_index]).ok());
    let next = ordered_ids
        .get(index + 1)
        .and_then(|next_id| neighbor_summary(book, next_id).ok());
    Ok(NoteNeighbors { previous, next })
}

fn neighbor_summary(book: &Book, id: &NoteId) -> CoreResult<NoteNeighborSummary> {
    let note = book.store.read_note(id)?;
    Ok(NoteNeighborSummary {
        id: note.id.to_string(),
        title: note.title,
        summary: note.summary,
    })
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

pub fn note_token_count_from_shared_tokenizer(text: &str) -> NoteTokenCount {
    let count = crate::text::tokenize(text).len();
    NoteTokenCount {
        count,
        method: NoteTokenCountMethod::SharedTokenizer,
        warning: count > 2_000,
    }
}

pub fn note_embedding_details(book: &Book, id: &str) -> CoreResult<NoteEmbeddingDetails> {
    let note_id = NoteId::parse(id)?;
    let note = book.store.read_note(&note_id)?;
    let expected = configured_model_fingerprint(&book.config.embedding)?;
    let path = layout::embedding_sidecar_path(&book.root, &note.id);
    let sidecar = match read_sidecar(&path) {
        Ok(sidecar) => sidecar,
        Err(CoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NoteEmbeddingDetails {
                status: "missing".to_string(),
                generated_at_unix_ms: None,
                model_id: Some(expected.model_id),
                dimensions: Some(expected.dimensions),
                summary_vector: None,
                full_note_vector: None,
            });
        }
        Err(error) => {
            return Ok(NoteEmbeddingDetails {
                status: format!("unreadable: {error}"),
                generated_at_unix_ms: None,
                model_id: Some(expected.model_id),
                dimensions: Some(expected.dimensions),
                summary_vector: None,
                full_note_vector: None,
            });
        }
    };
    let status = if sidecar.note_ulid != note.id.ulid() || !sidecar.is_compatible_with(&expected) {
        "incompatible"
    } else if !sidecar.summary_is_fresh(&note) || !sidecar.full_note_is_fresh(&note) {
        "stale"
    } else {
        "fresh"
    };
    Ok(NoteEmbeddingDetails {
        status: status.to_string(),
        generated_at_unix_ms: Some(sidecar.generated_at_unix_ms),
        model_id: Some(sidecar.model.model_id),
        dimensions: Some(sidecar.model.dimensions),
        summary_vector: sidecar.summary.as_ref().map(stored_vector),
        full_note_vector: sidecar.full_note.as_ref().map(stored_vector),
    })
}

fn stored_vector(stored: &StoredEmbedding) -> Vec<f32> {
    stored.vector.0.clone()
}

pub fn merge_notes(book: &Book, request: MergeNotesRequest) -> CoreResult<NoteDto> {
    let target_id = NoteId::parse(&request.target_note_id)?;
    let mut target = book.store.read_note(&target_id)?;
    let mut merged_sections = vec![target.body.trim().to_string()]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>();
    for source_id in &request.source_note_ids {
        if source_id == &request.target_note_id {
            continue;
        }
        let source = book.store.read_note(&NoteId::parse(source_id)?)?;
        if !source.summary.trim().is_empty() && target.summary.trim().is_empty() {
            target.summary = source.summary.clone();
        }
        merge_string_set(&mut target.categories, &source.categories);
        if target.location.is_none() {
            target.location = source.location.clone();
        }
        if !source.body.trim().is_empty() {
            merged_sections.push(source.body.trim().to_string());
        }
    }
    target.body = merged_sections.join("\n\n---\n\n");
    target.metadata = merge_metadata_for_note_change(target.metadata);
    merge_inline_categories(&mut target);
    ensure_categories_declared(book, &target.categories)?;
    book.save_note(&target)?;
    Ok(NoteDto::from_note(&target))
}

fn merge_string_set(target: &mut Vec<String>, source: &[String]) {
    for item in source {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

fn merge_metadata_for_note_change(mut metadata: Metadata) -> Metadata {
    metadata.dates.updated = Utc::now();
    metadata
}

pub fn split_note(book: &Book, request: SplitNoteRequest) -> CoreResult<(NoteDto, NoteDto)> {
    let note_id = NoteId::parse(&request.note_id)?;
    let mut first = book.store.read_note(&note_id)?;
    let split_at = request.split_at.min(first.body.len());
    if !first.body.is_char_boundary(split_at) {
        return Err(CoreError::InvalidBook(
            "split offset must be a UTF-8 character boundary".to_string(),
        ));
    }
    let second_body = first.body[split_at..].trim_start().to_string();
    first.body = first.body[..split_at].trim_end().to_string();
    first.metadata.dates.updated = Utc::now();
    book.save_note(&first)?;

    let mut second = book.new_note(
        first.object_type,
        request
            .second_title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("{} (split)", first.title)),
    )?;
    second.summary = first.summary.clone();
    second.body = second_body;
    second.categories = first.categories.clone();
    second.location = first.location.clone();
    second.prior = Some(PriorEdge {
        target: crate::model::PriorRef::Note(first.id.clone()),
        kind: crate::model::PriorKind::NewParagraph,
    });
    second.metadata.classification = first.metadata.classification.clone();
    second.metadata.lifecycle = first.metadata.lifecycle.clone();
    second.metadata.packs = first.metadata.packs.clone();
    second.metadata.kanban = first.metadata.kanban.clone();
    book.save_note(&second)?;

    Ok((NoteDto::from_note(&first), NoteDto::from_note(&second)))
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

    #[test]
    fn renders_note_markdown_with_comments_removed_and_clozes_hidden() {
        let (_dir, book) = book();
        let html = render_note_markdown(
            &book,
            None,
            Some("Keep %%private%% **public** and ||c1::hidden|hint||."),
            &|_, _| None,
        )
        .unwrap();

        assert!(html.contains("<strong>public</strong>"));
        assert!(!html.contains("private"));
        assert!(html.contains("class=\"syl-cloze\""));
        assert!(html.contains("data-hidden=\"hidden\""));
        assert!(html.contains(">hint</button>"));
    }

    #[test]
    fn note_neighbors_follow_book_order() {
        let (_dir, book) = book();
        create_category(&book, Category::new("chapter")).unwrap();
        let mut first = create_note(&book, ObjectType::Note, "first", None).unwrap();
        first.prior = Some(PriorEdge::starts_category("chapter"));
        first = update_note(&book, first).unwrap();
        let mut second = create_note(&book, ObjectType::Note, "second", None).unwrap();
        second.prior = Some(PriorEdge::follows(
            NoteId::parse(&first.id).unwrap(),
            crate::model::PriorKind::NewParagraph,
        ));
        second = update_note(&book, second).unwrap();

        let first_neighbors = note_neighbors(&book, &first.id).unwrap();
        assert!(first_neighbors.previous.is_none());
        assert_eq!(first_neighbors.next.unwrap().id, second.id);

        let second_neighbors = note_neighbors(&book, &second.id).unwrap();
        assert_eq!(second_neighbors.previous.unwrap().id, first.id);
        assert!(second_neighbors.next.is_none());
    }

    #[test]
    fn shared_token_count_warns_above_two_thousand_tokens() {
        let text = (0..2001)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let count = note_token_count_from_shared_tokenizer(&text);
        assert_eq!(count.count, 2001);
        assert_eq!(count.method, NoteTokenCountMethod::SharedTokenizer);
        assert!(count.warning);
    }

    #[test]
    fn merge_notes_concatenates_body_and_categories() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "target", None).unwrap();
        target.body = "Target body".into();
        target.categories = vec!["a".into()];
        target = update_note(&book, target).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "source", None).unwrap();
        source.body = "Source body".into();
        source.categories = vec!["b".into()];
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert!(merged.body.contains("Target body"));
        assert!(merged.body.contains("Source body"));
        assert!(merged.categories.contains(&"a".into()));
        assert!(merged.categories.contains(&"b".into()));
    }

    #[test]
    fn split_note_creates_second_note_after_first() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "whole", None).unwrap();
        note.body = "first half\n\nsecond half".into();
        note.categories = vec!["topic".into()];
        note = update_note(&book, note).unwrap();

        let (first, second) = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at: "first half".len(),
                second_title: Some("second".into()),
            },
        )
        .unwrap();

        assert_eq!(first.body, "first half");
        assert_eq!(second.title, "second");
        assert_eq!(second.body, "second half");
        assert_eq!(second.categories, vec!["topic".to_string()]);
        assert!(matches!(
            second.prior.unwrap().target,
            crate::model::PriorRef::Note(_)
        ));
    }
}
