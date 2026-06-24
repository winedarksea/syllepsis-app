//! Commands for retrieval: full search, the related carousel, and embedding diagnostics.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use syllepsis_core::app::search as app;
use syllepsis_core::graph_analysis::{GraphAnalysisRequest, GraphAnalysisResult};
use syllepsis_core::search::{EmbeddingDiagnostics, RelatedNote, SearchResults};
use syllepsis_core::storage::{Book, NoteStore};

use crate::state::{models_root_from_app_data, AppState};

/// A note stub from another book — title + id, enough to build a cross-book link.
#[derive(Debug, Serialize, Deserialize)]
pub struct CrossBookNote {
    pub book_name: String,
    pub book_path: String,
    pub note_id: String,
    pub title: String,
    pub summary: String,
}

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// Run a full search; `category_filter` (possibly empty) narrows the hits.
#[tauri::command]
pub fn search(
    state: State<AppState>,
    query: String,
    category_filter: Vec<String>,
) -> Result<SearchResults, String> {
    with_book!(state, book, {
        app::search(book, &query, &category_filter).map_err(|e| e.to_string())
    })
}

/// Notes related to `id` for the related carousel.
#[tauri::command]
pub fn related_notes(state: State<AppState>, id: String) -> Result<Vec<RelatedNote>, String> {
    with_book!(state, book, {
        app::related_notes(book, &id).map_err(|e| e.to_string())
    })
}

/// Embedding health report (near-duplicates, blind spots).
#[tauri::command]
pub fn embedding_diagnostics(state: State<AppState>) -> Result<EmbeddingDiagnostics, String> {
    with_book!(state, book, {
        app::embedding_diagnostics(book).map_err(|e| e.to_string())
    })
}

/// Build a semantic graph snapshot without blocking the Tauri event loop.
#[tauri::command]
pub async fn graph_analysis(
    app_handle: tauri::AppHandle,
    request: GraphAnalysisRequest,
) -> Result<GraphAnalysisResult, String> {
    use tauri::Manager;

    tauri::async_runtime::spawn_blocking(move || {
        app_handle.state::<AppState>().analyze_graph(&request)
    })
    .await
    .map_err(|error| format!("graph analysis worker failed: {error}"))?
}

/// Search notes across all tracked books (not just the open one).
/// Opens each tracked book temporarily; skips books that fail to open.
#[tauri::command]
pub fn search_across_books(
    app_handle: tauri::AppHandle,
    state: State<AppState>,
    query: String,
) -> Result<Vec<CrossBookNote>, String> {
    use tauri::Manager;

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;

    // Load tracked paths (reuse the JSON directly to avoid circular imports).
    #[derive(serde::Deserialize, Default)]
    struct TrackedBookPaths {
        paths: Vec<String>,
    }
    let tracked_path = app_data_dir.join("tracked-books.json");
    let tracked: TrackedBookPaths = if tracked_path.exists() {
        let content = std::fs::read_to_string(&tracked_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        TrackedBookPaths::default()
    };

    // Skip the currently-open book to avoid conflicts.
    let open_path = {
        let guard = state.book.lock().unwrap();
        guard.as_ref().map(|b| b.root.display().to_string())
    };

    let models_root = models_root_from_app_data(&app_data_dir);
    let mut results = Vec::new();
    let q = query.to_lowercase();

    for path_str in &tracked.paths {
        if open_path.as_deref() == Some(path_str.as_str()) {
            continue;
        }
        let book_path = PathBuf::from(path_str);
        let book = match Book::open(&book_path) {
            Ok(b) => b.with_models_root(models_root.clone()),
            Err(_) => continue,
        };
        let notes = match book.store.read_all_notes() {
            Ok(n) => n,
            Err(_) => continue,
        };
        for note in notes {
            if !note.metadata.is_visible_in_default_views() {
                continue;
            }
            if note.title.to_lowercase().contains(&q)
                || note.summary.to_lowercase().contains(&q)
                || note.body.to_lowercase().contains(&q)
            {
                results.push(CrossBookNote {
                    book_name: book.metadata.name.clone(),
                    book_path: path_str.clone(),
                    note_id: note.id.to_string(),
                    title: note.title.clone(),
                    summary: note.summary.chars().take(200).collect(),
                });
            }
        }
    }
    Ok(results)
}
