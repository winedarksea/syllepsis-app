//! Commands for retrieval: full search, the related carousel, and embedding diagnostics.

use tauri::State;

use syllepsis_core::app::search as app;
use syllepsis_core::search::{EmbeddingDiagnostics, RelatedNote, SearchResults};

use crate::state::AppState;

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
