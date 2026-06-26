//! Commands for commentary child objects.

use tauri::State;

use syllepsis_core::app::{commentary as app, dto::NoteDto};
use syllepsis_core::model::CommentaryKind;

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

#[tauri::command]
pub fn list_commentary(
    state: State<AppState>,
    parent_note_id: String,
    include_resolved: Option<bool>,
) -> Result<Vec<app::CommentarySummary>, String> {
    with_book!(state, book, {
        app::list_commentary(book, &parent_note_id, include_resolved.unwrap_or(false))
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn get_commentary(state: State<AppState>, commentary_id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::get_commentary(book, &commentary_id).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn create_commentary(
    state: State<AppState>,
    parent_note_id: String,
    kind: CommentaryKind,
    body: String,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::create_commentary(book, &parent_note_id, kind, &body)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn update_commentary(state: State<AppState>, commentary: NoteDto) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::update_commentary(book, commentary).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn apply_commentary(
    state: State<AppState>,
    commentary_id: String,
    options: Option<app::ApplyCommentaryOptions>,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::apply_commentary(book, &commentary_id, options.unwrap_or_default())
            .map_err(|error| error.to_string())?;
        let _ = state.local_ai.enqueue_note(book, updated.id.clone(), false);
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

#[tauri::command]
pub fn dismiss_commentary(
    state: State<AppState>,
    commentary_id: String,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::dismiss_commentary(book, &commentary_id).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn pin_commentary(state: State<AppState>, commentary_id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::pin_commentary(book, &commentary_id).map_err(|error| error.to_string())
    })
}
