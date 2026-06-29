//! Commands for note CRUD, the unsorted queue, and the continuous book view.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use syllepsis_core::app::{commands as app, dto::NoteDto, plugin as app_plugin};
use syllepsis_core::model::{NoteVisibility, ObjectType, PriorEdge};
use syllepsis_core::onnx::{self, ModelCache};
use syllepsis_core::sort::RenderItem;
use syllepsis_core::storage::NoteStore;

use crate::commands::plugins::PluginRuntime;
use crate::state::{models_root_from_app_data, AppState};

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// The full sorted book as a sequence of render items.
#[tauri::command]
pub fn book_view(state: State<AppState>) -> Result<Vec<RenderItem>, String> {
    with_book!(state, book, {
        app::book_view(book).map_err(|e| e.to_string())
    })
}

/// Notes awaiting categorization, newest first.
#[tauri::command]
pub fn unsorted_notes(state: State<AppState>) -> Result<Vec<NoteDto>, String> {
    with_book!(state, book, {
        app::unsorted_notes(book).map_err(|e| e.to_string())
    })
}

/// Fetch a single note by id string.
#[tauri::command]
pub fn get_note(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::get_note(book, &id).map_err(|e| e.to_string())
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderNoteMarkdownRequest {
    pub note_id: Option<String>,
    pub markdown: Option<String>,
}

#[tauri::command]
pub fn render_note_markdown(
    state: State<AppState>,
    plugins: State<PluginRuntime>,
    request: RenderNoteMarkdownRequest,
) -> Result<String, String> {
    with_book!(state, book, {
        plugins.host.set_book_root(Some(book.root.clone()));
        let disabled = plugins.disabled_ids.lock().unwrap().clone();
        app::render_note_markdown(
            book,
            request.note_id.as_deref(),
            request.markdown.as_deref(),
            &|lang, code| {
                app_plugin::run_render_plugin(
                    &plugins.host,
                    &plugins.registry,
                    &disabled,
                    lang,
                    code,
                )
                .ok()
            },
        )
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn note_neighbors(
    state: State<AppState>,
    note_id: String,
) -> Result<app::NoteNeighbors, String> {
    with_book!(state, book, {
        app::note_neighbors(book, &note_id).map_err(|e| e.to_string())
    })
}

/// Every visible note, title-sorted (backs the graph view).
#[tauri::command]
pub fn list_notes(
    state: State<AppState>,
    visibility: Option<NoteVisibility>,
) -> Result<Vec<NoteDto>, String> {
    with_book!(state, book, {
        app::list_notes_with_visibility(book, visibility.unwrap_or_default())
            .map_err(|e| e.to_string())
    })
}

/// Create a new note, optionally inheriting categories from `inherit_from`.
#[tauri::command]
pub fn create_note(
    state: State<AppState>,
    object_type: ObjectType,
    title: String,
    inherit_from: Option<String>,
    options: Option<app::CreateNoteOptions>,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let created = app::create_note_with_options(
            book,
            object_type,
            &title,
            inherit_from.as_deref(),
            options.unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, created.id.clone(), false);
        Ok(created)
    })
}

/// Persist edits to a note (bumps updated timestamp, folds inline #tags).
#[tauri::command]
pub fn update_note(state: State<AppState>, note: NoteDto) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::update_note(book, note).map_err(|e| e.to_string())?;
        let stored = book
            .store
            .read_note(&syllepsis_core::id::NoteId::parse(&updated.id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if syllepsis_core::embeddings::note_embedding_is_stale(book, &stored)
            .map_err(|e| e.to_string())?
        {
            state
                .local_ai
                .enqueue_note(book, updated.id.clone(), false)?;
        }
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Set (or clear) a note's sort position.
#[tauri::command]
pub fn set_prior(
    state: State<AppState>,
    id: String,
    prior: Option<PriorEdge>,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::set_prior(book, &id, prior).map_err(|e| e.to_string())
    })
}

/// Fork a note: new identity, lineage recorded.
#[tauri::command]
pub fn fork_note(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let forked = app::fork_note(book, &id).map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, forked.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(forked)
    })
}

/// Permanently delete a note by id.
#[tauri::command]
pub fn delete_note(state: State<AppState>, id: String) -> Result<(), String> {
    with_book!(state, book, {
        app::delete_note(book, &id).map_err(|e| e.to_string())
    })
}

/// Export the full book as a single linear markdown manuscript.
#[tauri::command]
pub fn export_markdown(state: State<AppState>) -> Result<String, String> {
    with_book!(state, book, {
        app::export_markdown(book).map_err(|e| e.to_string())
    })
}

/// Copy an external file into the book's `assets/` folder; returns the book-relative path.
#[tauri::command]
pub fn import_asset(state: State<AppState>, source_path: String) -> Result<String, String> {
    with_book!(state, book, {
        app::import_asset(book, &source_path).map_err(|e| e.to_string())
    })
}

/// Import a raster image or SVG as a first-class Picture/Drawing note.
#[tauri::command]
pub fn import_image_object(
    state: State<AppState>,
    source_path: String,
    title: Option<String>,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let imported = syllepsis_core::app::image_assets::import_image_object(
            book,
            &source_path,
            title.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        let _ = state
            .local_ai
            .enqueue_note(book, imported.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(imported)
    })
}

/// Serve a tracked image asset as a self-contained data URL.
#[tauri::command]
pub fn asset_data(state: State<AppState>, asset_uuid: String) -> Result<Option<String>, String> {
    with_book!(state, book, {
        let Some((path, media_type)) =
            syllepsis_core::app::image_assets::asset_file(book, &asset_uuid)
                .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        use base64::Engine as _;
        Ok(Some(format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
    })
}

/// Read the CSV companion file for a Table note. Returns an empty 5×3 grid if absent.
#[tauri::command]
pub fn read_table_data(
    state: State<AppState>,
    note_id: String,
) -> Result<Vec<Vec<String>>, String> {
    with_book!(state, book, {
        app::read_table_data(book, &note_id).map_err(|e| e.to_string())
    })
}

/// Write the CSV companion file for a Table note.
#[tauri::command]
pub fn save_table_data(
    state: State<AppState>,
    note_id: String,
    rows: Vec<Vec<String>>,
) -> Result<(), String> {
    with_book!(state, book, {
        app::save_table_data(book, &note_id, rows).map_err(|e| e.to_string())
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteTokenCountRequest {
    pub note_id: Option<String>,
    pub text: Option<String>,
}

#[tauri::command]
pub fn note_token_count(
    app_handle: AppHandle,
    state: State<AppState>,
    request: NoteTokenCountRequest,
) -> Result<app::NoteTokenCount, String> {
    let (text, embedding_model_id) = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        let text = match (&request.text, &request.note_id) {
            (Some(text), _) => text.clone(),
            (None, Some(note_id)) => {
                let note = app::get_note(book, note_id).map_err(|e| e.to_string())?;
                note.body
            }
            (None, None) => String::new(),
        };
        (text, book.config.embedding.model_id.clone())
    };
    if let Ok(exact) = exact_embedding_token_count(&app_handle, &embedding_model_id, &text) {
        return Ok(exact);
    }
    Ok(app::note_token_count_from_shared_tokenizer(&text))
}

fn exact_embedding_token_count(
    app_handle: &AppHandle,
    embedding_model_id: &str,
    text: &str,
) -> Result<app::NoteTokenCount, String> {
    let manifest = onnx::builtin(embedding_model_id)
        .ok_or_else(|| format!("unknown embedding model {embedding_model_id}"))?;
    let tokenizer_file = manifest
        .tokenizer_file()
        .ok_or_else(|| format!("embedding model {embedding_model_id} has no tokenizer"))?;
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir: {error}"))?;
    let cache = ModelCache::new(models_root_from_app_data(&app_data_dir));
    if !cache.is_cached(&manifest) {
        return Err(format!(
            "embedding model {embedding_model_id} is not cached"
        ));
    }
    let tokenizer = onnx::ModelTokenizer::from_file(&cache.file_path(&manifest, tokenizer_file))
        .map_err(|error| error.to_string())?;
    let count = tokenizer
        .encode(text, true)
        .map_err(|error| error.to_string())?
        .len();
    Ok(app::NoteTokenCount {
        count,
        method: app::NoteTokenCountMethod::EmbeddingTokenizer,
        warning: count > 2_000,
    })
}

#[tauri::command]
pub fn note_embedding_details(
    state: State<AppState>,
    note_id: String,
) -> Result<app::NoteEmbeddingDetails, String> {
    with_book!(state, book, {
        app::note_embedding_details(book, &note_id).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn merge_notes(
    state: State<AppState>,
    request: app::MergeNotesRequest,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::merge_notes(book, request).map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, updated.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitNoteResult {
    pub first: NoteDto,
    pub second: NoteDto,
}

#[tauri::command]
pub fn split_note(
    state: State<AppState>,
    request: app::SplitNoteRequest,
) -> Result<SplitNoteResult, String> {
    with_book!(state, book, {
        let (first, second) = app::split_note(book, request).map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_note(book, first.id.clone(), false);
        let _ = state.local_ai.enqueue_note(book, second.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(SplitNoteResult { first, second })
    })
}

/// Export the full book as a single HTML document, writing it to `path`. Fenced code blocks whose
/// language is claimed by a code-block-renderer plugin are rendered via that plugin.
#[tauri::command]
pub fn export_html(
    state: State<AppState>,
    plugins: State<PluginRuntime>,
    path: String,
) -> Result<(), String> {
    with_book!(state, book, {
        plugins.host.set_book_root(Some(book.root.clone()));
        let disabled = plugins.disabled_ids.lock().unwrap().clone();
        let html = app::export_html(book, &|lang, code| {
            app_plugin::run_render_plugin(&plugins.host, &plugins.registry, &disabled, lang, code)
                .ok()
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(&path, html).map_err(|e| format!("write HTML: {e}"))
    })
}

/// Export the full book as Markdown, writing it to `path`.
#[tauri::command]
pub fn export_markdown_to_file(state: State<AppState>, path: String) -> Result<(), String> {
    with_book!(state, book, {
        let md = app::export_markdown(book).map_err(|e| e.to_string())?;
        std::fs::write(&path, md).map_err(|e| format!("write Markdown: {e}"))
    })
}

/// Aggregate statistics about the open book.
#[tauri::command]
pub fn book_stats(state: State<AppState>) -> Result<app::BookStats, String> {
    with_book!(state, book, {
        app::book_stats(book).map_err(|e| e.to_string())
    })
}
