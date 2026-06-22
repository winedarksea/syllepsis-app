//! Commands for note CRUD, the unsorted queue, and the continuous book view.

use tauri::State;

use syllepsis_core::app::{commands as app, dto::NoteDto};
use syllepsis_core::model::{ObjectType, PriorEdge};
use syllepsis_core::sort::RenderItem;

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

/// Every visible note, title-sorted (backs the graph view).
#[tauri::command]
pub fn list_notes(state: State<AppState>) -> Result<Vec<NoteDto>, String> {
    with_book!(state, book, {
        app::list_notes(book).map_err(|e| e.to_string())
    })
}

/// Create a new note, optionally inheriting categories from `inherit_from`.
#[tauri::command]
pub fn create_note(
    state: State<AppState>,
    object_type: ObjectType,
    title: String,
    inherit_from: Option<String>,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::create_note(book, object_type, &title, inherit_from.as_deref())
            .map_err(|e| e.to_string())
    })
}

/// Persist edits to a note (bumps updated timestamp, folds inline #tags).
#[tauri::command]
pub fn update_note(state: State<AppState>, note: NoteDto) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::update_note(book, note).map_err(|e| e.to_string())
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
        app::fork_note(book, &id).map_err(|e| e.to_string())
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
