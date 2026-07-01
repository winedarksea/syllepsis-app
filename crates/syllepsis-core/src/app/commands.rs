//! The application command surface: framework-agnostic operations over a [`Book`].
//!
//! These are the functions the Tauri shell will expose as `#[tauri::command]` wrappers (and a
//! PWA worker can call directly). Keeping the logic here — not in the Tauri layer — means it
//! is unit-testable without a running app and shared across delivery targets (platform-infra.md
//! "share as much code as possible").

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use crate::app::dto::NoteDto;
use crate::embeddings::repository::configured_model_fingerprint;
use crate::embeddings::{read_sidecar, StoredEmbedding};
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::dialect;
use crate::model::classification::Priority;
use crate::model::metadata::{FlexDate, LockMode, Metadata};
use crate::model::{
    Category, ClassificationKind, Note, NoteStatus, NoteVisibility, ObjectType, PriorEdge, PriorRef,
};
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
    pub hidden_notes: usize,
    pub archived_notes: usize,
    pub starred_notes: usize,
    pub notes_by_type: HashMap<String, usize>,
    pub notes_by_category: HashMap<String, usize>,
    pub total_categories: usize,
    pub notes_with_location: usize,
    pub avg_word_count: usize,
    pub notes_with_attachments: usize,
    pub ai_generated_notes: usize,
    pub uncategorized_notes: usize,
    pub created_this_week: usize,
    pub updated_this_week: usize,
    pub overdue_tasks: usize,
    pub core_priority_notes: usize,
    pub scheduled_for_deletion: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateNoteOptions {
    pub vanishing: bool,
    pub vanish_days: Option<u32>,
    pub classification: Option<ClassificationKind>,
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
    if note.object_type == ObjectType::Commentary {
        return false;
    }
    match visibility {
        NoteVisibility::Active => note.metadata.is_visible_in_default_views(),
        NoteVisibility::Archived => {
            note.metadata.lifecycle.archived
                && !note.metadata.lifecycle.hidden
                && note.metadata.lifecycle.marked_for_deletion_at.is_none()
        }
        NoteVisibility::Trash => {
            !note.metadata.lifecycle.hidden
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
    let categories = all_categories(book)?;
    let now = Utc::now();
    let week_ago = now - Duration::days(7);

    let mut stats = BookStats {
        total_notes: 0,
        sorted_notes: 0,
        unsorted_notes: 0,
        hidden_notes: 0,
        archived_notes: 0,
        starred_notes: 0,
        notes_by_type: HashMap::new(),
        notes_by_category: HashMap::new(),
        total_categories: categories.len(),
        notes_with_location: 0,
        avg_word_count: 0,
        notes_with_attachments: 0,
        ai_generated_notes: 0,
        uncategorized_notes: 0,
        created_this_week: 0,
        updated_this_week: 0,
        overdue_tasks: 0,
        core_priority_notes: 0,
        scheduled_for_deletion: 0,
    };

    let mut total_words: usize = 0;

    for note in &all_notes {
        if note.object_type == ObjectType::Commentary {
            continue;
        }
        if note.metadata.lifecycle.marked_for_deletion_at.is_some() {
            stats.scheduled_for_deletion += 1;
            continue;
        }
        stats.total_notes += 1;
        if note.is_sorted() {
            stats.sorted_notes += 1;
        } else {
            stats.unsorted_notes += 1;
        }
        if note.metadata.lifecycle.hidden {
            stats.hidden_notes += 1;
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

        total_words += note.body.split_whitespace().count();
        if note.asset.is_some() {
            stats.notes_with_attachments += 1;
        }
        if note.metadata.authorship.ai_generated {
            stats.ai_generated_notes += 1;
        }
        if note.categories.is_empty() {
            stats.uncategorized_notes += 1;
        }
        if note.metadata.dates.created >= week_ago {
            stats.created_this_week += 1;
        }
        if note.metadata.dates.updated >= week_ago {
            stats.updated_this_week += 1;
        }
        if note.metadata.classification.priority == Priority::Core {
            stats.core_priority_notes += 1;
        }
        if is_overdue(note, now) {
            stats.overdue_tasks += 1;
        }
    }

    stats.avg_word_count = if stats.total_notes > 0 {
        total_words / stats.total_notes
    } else {
        0
    };

    Ok(stats)
}

/// A note is an overdue task when it carries an absolute `due` date in the past and hasn't been
/// completed. Relative/anchor-based due dates are left undated here, matching the same
/// simplification `graph_analysis::resolve_note_ms` already makes.
fn is_overdue(note: &Note, now: DateTime<Utc>) -> bool {
    let Some(due_date) = note.metadata.dates.due.as_ref().and_then(|d| d.date) else {
        return false;
    };
    if note.metadata.status == Some(NoteStatus::Done) || note.metadata.dates.completed.is_some() {
        return false;
    }
    let Some(due_midnight) = due_date.and_hms_opt(0, 0, 0) else {
        return false;
    };
    let due_utc: DateTime<Utc> = chrono::TimeZone::from_utc_datetime(&Utc, &due_midnight);
    due_utc < now
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
    hydrate_categories_from_note_metadata(book)?;
    let mut categories = book.store.categories()?;
    categories.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(categories)
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
    if object_type == ObjectType::Note {
        if let Some(classification) = options.classification {
            note.metadata.classification.kind = classification;
            note.body = starter_template_for_classification(classification).to_string();
            book.save_note(&note)?;
        }
    }
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

fn starter_template_for_classification(classification: ClassificationKind) -> &'static str {
    match classification {
        ClassificationKind::Todo => "- [ ] ",
        ClassificationKind::Qa => "question: \nanswer: \n",
        ClassificationKind::Quote => "> Quote text\n\nSource: \n",
        ClassificationKind::Reference => "Author. (Year). Title. Source. URL.\n",
        ClassificationKind::Code => "```text\n\n```\n",
        _ => "",
    }
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
            crate::app::commentary::create_commentary(
                book,
                stored.id.as_str(),
                crate::model::CommentaryKind::Proposal,
                &note.body,
            )?;
            note.body = stored.body.clone();
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
    cleanup_unused_default_categories(book)?;
    Ok(NoteDto::from_note(&note))
}

/// Metadata-only workflow status update for Kanban interactions. This avoids sending a stale body
/// copy back through `update_note` when the board only needs to change status/date metadata.
pub fn set_note_workflow_status(
    book: &Book,
    id: &str,
    status: Option<NoteStatus>,
    today_date: &str,
) -> CoreResult<NoteDto> {
    let today = NaiveDate::parse_from_str(today_date, "%Y-%m-%d").map_err(|error| {
        CoreError::InvalidBook(format!("invalid workflow date '{today_date}': {error}"))
    })?;
    let note_id = NoteId::parse(id)?;
    let mut note = book.store.read_note(&note_id)?;
    note.metadata.status = status;
    if status == Some(NoteStatus::Active) && note.metadata.dates.started.is_none() {
        note.metadata.dates.started = Some(FlexDate {
            date: Some(today),
            ..Default::default()
        });
    }
    if status == Some(NoteStatus::Done) && note.metadata.dates.completed.is_none() {
        note.metadata.dates.completed = Some(FlexDate {
            date: Some(today),
            ..Default::default()
        });
    }
    note.metadata.dates.updated = Utc::now();
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Declare a category file for any category referenced by a note that does not yet have one.
/// Without this, a `#tag` added to a note only lives in the note's frontmatter and never appears
/// in the sidebar (which lists declared categories). Existing category files are left untouched so
/// user customizations (icon, long name, privacy) survive.
fn ensure_categories_declared(book: &Book, categories: &[String]) -> CoreResult<()> {
    for name in categories {
        if name.trim().is_empty() {
            continue;
        }
        match book.store.read_category(name) {
            Ok(_) => {}
            Err(CoreError::NotFound(_)) => {
                book.store.write_category(&Category::new(name.clone()))?
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn hydrate_categories_from_note_metadata(book: &Book) -> CoreResult<()> {
    let mut referenced_categories = BTreeSet::new();
    for note in book.store.read_all_notes()? {
        referenced_categories.extend(
            note.categories
                .into_iter()
                .filter(|category| !category.trim().is_empty()),
        );
        if let Some(PriorRef::Category(category)) = note.prior.map(|prior| prior.target) {
            if !category.trim().is_empty() {
                referenced_categories.insert(category);
            }
        }
    }
    let referenced_categories: Vec<String> = referenced_categories.into_iter().collect();
    ensure_categories_declared(book, &referenced_categories)
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
    crate::app::commentary::delete_parent_commentary_now(book, id)?;
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

/// Delete a category only when no notes or category hierarchy edges still depend on it.
pub fn delete_category(book: &Book, name: &str) -> CoreResult<()> {
    book.store.read_category(name)?;
    let usage = category_usage(book, name)?;
    if usage.has_any_reference() {
        return Err(CoreError::InvalidBook(format!(
            "category '{name}' is still used by notes or category structure"
        )));
    }
    book.store.delete_category(name)
}

fn cleanup_unused_default_categories(book: &Book) -> CoreResult<()> {
    for category in book.store.categories()? {
        if is_auto_deletable_category(book, &category)? {
            book.store.delete_category(&category.name)?;
        }
    }
    Ok(())
}

fn is_auto_deletable_category(book: &Book, category: &Category) -> CoreResult<bool> {
    Ok(!category_has_user_meaningful_fields(category)
        && !category_usage(book, &category.name)?.has_any_reference())
}

fn category_has_user_meaningful_fields(category: &Category) -> bool {
    let display_name = category.long_name.trim();
    (!display_name.is_empty() && display_name != category.name)
        || category.icon.is_some()
        || category.location.is_some()
        || category.region.is_some()
        || category.parent.is_some()
        || category.hidden
        || category.exclude_from_search
        || category.exclude_from_publish
}

struct CategoryUsage {
    note_categories: usize,
    prior_targets: usize,
    child_categories: usize,
}

impl CategoryUsage {
    fn has_any_reference(&self) -> bool {
        self.note_categories > 0 || self.prior_targets > 0 || self.child_categories > 0
    }
}

fn category_usage(book: &Book, name: &str) -> CoreResult<CategoryUsage> {
    let mut usage = CategoryUsage {
        note_categories: 0,
        prior_targets: 0,
        child_categories: 0,
    };

    for note in book.store.read_all_notes()? {
        if note.categories.iter().any(|category| category == name) {
            usage.note_categories += 1;
        }
        if matches!(note.prior.as_ref().map(|prior| &prior.target), Some(PriorRef::Category(category)) if category == name)
        {
            usage.prior_targets += 1;
        }
    }

    for category in book.store.categories()? {
        if category.parent.as_deref() == Some(name) {
            usage.child_categories += 1;
        }
    }

    Ok(usage)
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
    fn workflow_status_update_preserves_note_content_and_stamps_missing_dates() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "task", None).unwrap();
        note.body = "do the careful thing".into();
        note.categories = vec!["work".into()];
        let note = update_note(&book, note).unwrap();

        let active =
            set_note_workflow_status(&book, &note.id, Some(NoteStatus::Active), "2026-06-30")
                .unwrap();
        assert_eq!(active.body, "do the careful thing");
        assert_eq!(active.categories, vec!["work"]);
        assert_eq!(active.metadata.status, Some(NoteStatus::Active));
        assert_eq!(
            active.metadata.dates.started.unwrap().date,
            Some(NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
        );

        let done = set_note_workflow_status(&book, &note.id, Some(NoteStatus::Done), "2026-07-01")
            .unwrap();
        assert_eq!(done.metadata.status, Some(NoteStatus::Done));
        assert_eq!(
            done.metadata.dates.started.unwrap().date,
            Some(NaiveDate::from_ymd_opt(2026, 6, 30).unwrap())
        );
        assert_eq!(
            done.metadata.dates.completed.unwrap().date,
            Some(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap())
        );

        let cleared = set_note_workflow_status(&book, &note.id, None, "2026-07-02").unwrap();
        assert_eq!(cleared.metadata.status, None);
        assert!(cleared.metadata.dates.started.is_some());
        assert!(cleared.metadata.dates.completed.is_some());
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
    fn all_categories_hydrates_legacy_note_category_metadata() {
        let (_dir, book) = book();
        let mut tagged = book.new_note(ObjectType::Note, "tagged").unwrap();
        tagged.categories = vec!["legacy".into()];
        book.save_note(&tagged).unwrap();

        let mut sorted = book.new_note(ObjectType::Note, "sorted").unwrap();
        sorted.prior = Some(PriorEdge::starts_category("chapter"));
        book.save_note(&sorted).unwrap();

        assert!(book.store.read_category("legacy").is_err());
        assert!(book.store.read_category("chapter").is_err());

        let declared: Vec<String> = all_categories(&book)
            .unwrap()
            .into_iter()
            .map(|category| category.name)
            .collect();

        assert_eq!(declared, vec!["chapter".to_string(), "legacy".to_string()]);
        assert!(book.store.read_category("legacy").is_ok());
        assert!(book.store.read_category("chapter").is_ok());
        assert_eq!(book_stats(&book).unwrap().total_categories, 2);
    }

    #[test]
    fn book_stats_computes_new_overview_counters() {
        let (_dir, book) = book();

        // Core-priority, AI-generated, uncategorized note with a body for word count.
        let mut core_note = create_note(&book, ObjectType::Note, "core", None).unwrap();
        core_note.body = "one two three four five".to_string();
        core_note.metadata.classification.priority = Priority::Core;
        core_note.metadata.authorship.ai_generated = true;
        update_note(&book, core_note).unwrap();

        // Categorized note.
        let mut categorized = create_note(&book, ObjectType::Note, "categorized", None).unwrap();
        categorized.categories = vec!["science".into()];
        update_note(&book, categorized).unwrap();

        // Overdue, not-done task.
        let mut overdue = create_note(&book, ObjectType::Note, "overdue", None).unwrap();
        overdue.metadata.dates.due = Some(FlexDate {
            date: NaiveDate::from_ymd_opt(2020, 1, 1),
            ..Default::default()
        });
        update_note(&book, overdue).unwrap();

        // Trashed note: soft-deleted and pending purge, not merely scheduled to self-destruct.
        let trashed = create_note(&book, ObjectType::Note, "trashed", None).unwrap();
        crate::app::lifecycle::request_deletion(&book, &trashed.id).unwrap();

        let stats = book_stats(&book).unwrap();
        assert_eq!(stats.core_priority_notes, 1);
        assert_eq!(stats.ai_generated_notes, 1);
        assert_eq!(stats.scheduled_for_deletion, 1);
        assert_eq!(stats.overdue_tasks, 1);
        // core_note + overdue are uncategorized; categorized has a category; trashed is excluded entirely.
        assert_eq!(stats.uncategorized_notes, 2);
        assert_eq!(stats.created_this_week, 3);
        assert_eq!(stats.updated_this_week, 3);
        assert!(stats.avg_word_count > 0);
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
    fn removing_the_last_inline_reference_prunes_default_category() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "first", None).unwrap();
        note.body = "Draft #ca".into();
        let mut note = update_note(&book, note).unwrap();
        assert!(book.store.read_category("ca").is_ok());

        note.body = "Draft".into();
        let note = update_note(&book, note).unwrap();

        assert!(!note.categories.contains(&"ca".to_string()));
        assert!(book.store.read_category("ca").is_err());
    }

    #[test]
    fn cleanup_preserves_custom_empty_categories() {
        let (_dir, book) = book();
        let customizations = [
            {
                let mut category = Category::new("custom_name");
                category.long_name = "Custom Name".into();
                category
            },
            {
                let mut category = Category::new("icon");
                category.icon = Some("*".into());
                category
            },
            {
                let mut category = Category::new("location");
                category.location = Some("earth/47.6,-122.3".into());
                category
            },
            {
                let mut category = Category::new("region");
                category.region = Some(crate::model::SpatialRegion::SvgElement {
                    element_id: "kitchen".into(),
                });
                category
            },
            {
                let mut category = Category::new("child");
                category.parent = Some("parent".into());
                category
            },
            {
                let mut category = Category::new("private");
                category.hidden = true;
                category
            },
        ];
        for category in customizations {
            create_category(&book, category).unwrap();
        }

        let mut note = create_note(&book, ObjectType::Note, "first", None).unwrap();
        note.body = "Trigger cleanup".into();
        update_note(&book, note).unwrap();

        for name in [
            "custom_name",
            "icon",
            "location",
            "region",
            "child",
            "private",
        ] {
            assert!(book.store.read_category(name).is_ok(), "{name} was pruned");
        }
    }

    #[test]
    fn delete_category_rejects_note_and_structure_references() {
        let (_dir, book) = book();
        create_category(&book, Category::new("used")).unwrap();
        let mut tagged = create_note(&book, ObjectType::Note, "tagged", None).unwrap();
        tagged.categories = vec!["used".into()];
        update_note(&book, tagged).unwrap();

        assert!(matches!(
            delete_category(&book, "used").unwrap_err(),
            CoreError::InvalidBook(_)
        ));

        create_category(&book, Category::new("chapter")).unwrap();
        let mut sorted = create_note(&book, ObjectType::Note, "sorted", None).unwrap();
        sorted.prior = Some(PriorEdge::starts_category("chapter"));
        update_note(&book, sorted).unwrap();

        assert!(matches!(
            delete_category(&book, "chapter").unwrap_err(),
            CoreError::InvalidBook(_)
        ));

        create_category(&book, Category::new("parent")).unwrap();
        let mut child = Category::new("child");
        child.parent = Some("parent".into());
        create_category(&book, child).unwrap();

        assert!(matches!(
            delete_category(&book, "parent").unwrap_err(),
            CoreError::InvalidBook(_)
        ));
    }

    #[test]
    fn delete_category_allows_protected_empty_category() {
        let (_dir, book) = book();
        let mut category = Category::new("intentional");
        category.exclude_from_publish = true;
        create_category(&book, category).unwrap();

        delete_category(&book, "intentional").unwrap();

        assert!(book.store.read_category("intentional").is_err());
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
    fn new_note_defaults_to_note_classification() {
        let (_dir, book) = book();
        let note = create_note(&book, ObjectType::Note, "plain", None).unwrap();
        assert_eq!(note.object_type, ObjectType::Note);
        assert_eq!(note.metadata.classification.kind, ClassificationKind::Note);
        assert!(note.body.is_empty());
    }

    #[test]
    fn note_classification_shortcuts_seed_editable_templates() {
        let cases = [
            (ClassificationKind::Todo, "- [ ] "),
            (ClassificationKind::Qa, "question: \nanswer: \n"),
            (ClassificationKind::Quote, "> Quote text\n\nSource: \n"),
            (
                ClassificationKind::Reference,
                "Author. (Year). Title. Source. URL.\n",
            ),
            (ClassificationKind::Code, "```text\n\n```\n"),
        ];
        let (_dir, book) = book();

        for (classification, expected_body) in cases {
            let title = format!("{classification:?}");
            let note = create_note_with_options(
                &book,
                ObjectType::Note,
                &title,
                None,
                CreateNoteOptions {
                    classification: Some(classification),
                    ..CreateNoteOptions::default()
                },
            )
            .unwrap();
            assert_eq!(note.object_type, ObjectType::Note);
            assert_eq!(note.metadata.classification.kind, classification);
            assert_eq!(note.body, expected_body);
        }
    }

    #[test]
    fn table_creation_still_creates_csv_sidecar() {
        let (_dir, book) = book();
        let table = create_note_with_options(
            &book,
            ObjectType::Table,
            "data",
            None,
            CreateNoteOptions {
                classification: Some(ClassificationKind::Todo),
                ..CreateNoteOptions::default()
            },
        )
        .unwrap();
        assert_eq!(table.object_type, ObjectType::Table);
        assert_eq!(table.metadata.classification.kind, ClassificationKind::Note);
        let table_id = NoteId::parse(&table.id).unwrap();
        assert!(layout::table_companion_csv_path(&book.root, &table_id).exists());
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
                classification: None,
            },
        )
        .unwrap();
        assert!(note.metadata.lifecycle.vanish_at.is_some());
    }

    #[test]
    fn locked_note_direct_body_edit_becomes_commentary_proposal() {
        let (_dir, book) = book();
        let note = create_note(&book, ObjectType::Note, "protected", None).unwrap();
        crate::app::lifecycle::set_note_lock(&book, &note.id, LockMode::UnlockDelay).unwrap();

        let mut edit = get_note(&book, &note.id).unwrap();
        edit.body = "a direct overwrite that should be refused".into();
        let updated = update_note(&book, edit).unwrap();
        assert_eq!(updated.body, "");
        let commentary = book.read_all_commentary_notes().unwrap();
        assert_eq!(commentary.len(), 1);
        assert_eq!(
            commentary[0].body,
            "a direct overwrite that should be refused"
        );
        assert_eq!(
            commentary[0].commentary.as_ref().unwrap().status,
            crate::model::CommentaryStatus::Locked
        );

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
    fn fork_note_copies_saved_body_without_changing_source() {
        let (_dir, book) = book();
        let mut source = create_note(&book, ObjectType::Note, "source", None).unwrap();
        source.body = "saved source body".into();
        let source = update_note(&book, source).unwrap();

        let forked = fork_note(&book, &source.id).unwrap();
        let stored_source = get_note(&book, &source.id).unwrap();

        assert_eq!(stored_source.body, "saved source body");
        assert_eq!(forked.body, "saved source body");
        assert_eq!(forked.title, "Duplicate of source");
        assert_ne!(forked.id, source.id);
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

    // ── merge_notes ──────────────────────────────────────────────────────────

    #[test]
    fn merge_notes_uses_hr_separator_between_sections() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        target.body = "first".into();
        target = update_note(&book, target).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "s", None).unwrap();
        source.body = "second".into();
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert_eq!(merged.body, "first\n\n---\n\nsecond");
    }

    #[test]
    fn merge_notes_skips_empty_source_body() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        target.body = "content".into();
        target = update_note(&book, target).unwrap();
        let source = create_note(&book, ObjectType::Note, "empty", None).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert_eq!(merged.body, "content");
    }

    #[test]
    fn merge_notes_adopts_source_summary_when_target_has_none() {
        let (_dir, book) = book();
        let target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "s", None).unwrap();
        source.summary = "source summary".into();
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert_eq!(merged.summary, "source summary");
    }

    #[test]
    fn merge_notes_keeps_target_summary_when_set() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        target.summary = "target summary".into();
        target = update_note(&book, target).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "s", None).unwrap();
        source.summary = "source summary".into();
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert_eq!(merged.summary, "target summary");
    }

    #[test]
    fn merge_notes_adopts_source_location_when_target_has_none() {
        let (_dir, book) = book();
        let target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "s", None).unwrap();
        source.location = Some("Paris".into());
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        assert_eq!(merged.location.as_deref(), Some("Paris"));
    }

    #[test]
    fn merge_notes_skips_when_source_id_equals_target_id() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "n", None).unwrap();
        note.body = "solo".into();
        note = update_note(&book, note).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: note.id.clone(),
                source_note_ids: vec![note.id.clone()],
            },
        )
        .unwrap();

        assert_eq!(merged.body, "solo");
    }

    #[test]
    fn merge_notes_deduplicates_categories() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        target.categories = vec!["shared".into(), "only-target".into()];
        target = update_note(&book, target).unwrap();
        let mut source = create_note(&book, ObjectType::Note, "s", None).unwrap();
        source.categories = vec!["shared".into(), "only-source".into()];
        source = update_note(&book, source).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![source.id],
            },
        )
        .unwrap();

        let shared_count = merged
            .categories
            .iter()
            .filter(|c| c.as_str() == "shared")
            .count();
        assert_eq!(shared_count, 1);
        assert!(merged.categories.contains(&"only-target".to_string()));
        assert!(merged.categories.contains(&"only-source".to_string()));
    }

    #[test]
    fn merge_notes_combines_multiple_sources() {
        let (_dir, book) = book();
        let mut target = create_note(&book, ObjectType::Note, "t", None).unwrap();
        target.body = "A".into();
        target = update_note(&book, target).unwrap();
        let mut s1 = create_note(&book, ObjectType::Note, "s1", None).unwrap();
        s1.body = "B".into();
        s1 = update_note(&book, s1).unwrap();
        let mut s2 = create_note(&book, ObjectType::Note, "s2", None).unwrap();
        s2.body = "C".into();
        s2 = update_note(&book, s2).unwrap();

        let merged = merge_notes(
            &book,
            MergeNotesRequest {
                target_note_id: target.id.clone(),
                source_note_ids: vec![s1.id, s2.id],
            },
        )
        .unwrap();

        assert!(merged.body.contains("A"));
        assert!(merged.body.contains("B"));
        assert!(merged.body.contains("C"));
    }

    // ── fork_note ─────────────────────────────────────────────────────────────

    #[test]
    fn fork_note_records_lineage_in_metadata() {
        let (_dir, book) = book();
        let source = create_note(&book, ObjectType::Note, "original", None).unwrap();

        let forked = fork_note(&book, &source.id).unwrap();

        let fork_info = forked.metadata.fork.as_ref().unwrap();
        assert_eq!(fork_info.forked_from.to_string(), source.id);
    }

    #[test]
    fn fork_note_inherits_categories_and_summary() {
        let (_dir, book) = book();
        let mut source = create_note(&book, ObjectType::Note, "src", None).unwrap();
        source.summary = "a summary".into();
        source.categories = vec!["research".into()];
        let source = update_note(&book, source).unwrap();

        let forked = fork_note(&book, &source.id).unwrap();

        assert_eq!(forked.summary, "a summary");
        assert_eq!(forked.categories, vec!["research".to_string()]);
    }

    // ── split_note ────────────────────────────────────────────────────────────

    #[test]
    fn split_note_uses_original_title_with_split_suffix_when_no_second_title() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "whole", None).unwrap();
        note.body = "first\n\nsecond".into();
        note = update_note(&book, note).unwrap();

        let (_, second) = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at: "first".len(),
                second_title: None,
            },
        )
        .unwrap();

        assert_eq!(second.title, "whole (split)");
    }

    #[test]
    fn split_note_clamps_offset_beyond_body_length() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "n", None).unwrap();
        note.body = "short".into();
        note = update_note(&book, note).unwrap();

        let (first, second) = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at: 9999,
                second_title: Some("tail".into()),
            },
        )
        .unwrap();

        assert_eq!(first.body, "short");
        assert_eq!(second.body, "");
    }

    #[test]
    fn split_note_second_inherits_summary_and_location() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "n", None).unwrap();
        note.body = "part one\n\npart two".into();
        note.summary = "the summary".into();
        note.location = Some("London".into());
        note = update_note(&book, note).unwrap();

        let (_, second) = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at: "part one".len(),
                second_title: Some("two".into()),
            },
        )
        .unwrap();

        assert_eq!(second.summary, "the summary");
        assert_eq!(second.location.as_deref(), Some("London"));
    }

    #[test]
    fn split_note_rejects_non_char_boundary() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "n", None).unwrap();
        note.body = "café".into();
        note = update_note(&book, note).unwrap();

        // byte 4 is the second byte of 'é' (U+00E9 encodes as 0xC3 0xA9), not a boundary
        let result = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at: 4,
                second_title: None,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn split_note_trims_whitespace_at_split_boundary() {
        let (_dir, book) = book();
        let mut note = create_note(&book, ObjectType::Note, "n", None).unwrap();
        note.body = "first   \n\n   second".into();
        note = update_note(&book, note).unwrap();

        let split_at = "first   ".len();
        let (first, second) = split_note(
            &book,
            SplitNoteRequest {
                note_id: note.id.clone(),
                split_at,
                second_title: Some("s".into()),
            },
        )
        .unwrap();

        assert_eq!(first.body, "first");
        assert_eq!(second.body, "second");
    }
}
