//! Commands for the privacy & lifecycle policy (Phase 6, privacy-security.md): private/archived/
//! locked toggles, the delayed-deletion flow with its purge, and the centralized policy overview.

use tauri::State;

use syllepsis_core::app::dto::NoteDto;
use syllepsis_core::app::lifecycle::{self as app, PolicyOverview};
use syllepsis_core::model::LockMode;

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

/// The "what is restricted in this book" snapshot for the policy panel.
#[tauri::command]
pub fn policy_overview(state: State<AppState>) -> Result<PolicyOverview, String> {
    with_book!(state, book, {
        app::policy_overview(book).map_err(|e| e.to_string())
    })
}

/// Toggle a note's private flag (drops it from default views, RAG, and publish).
#[tauri::command]
pub fn set_note_private(
    state: State<AppState>,
    id: String,
    private: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::set_note_private(book, &id, private).map_err(|e| e.to_string())
    })
}

/// Toggle a note's archived flag (hidden from default views, reversible).
#[tauri::command]
pub fn set_note_archived(
    state: State<AppState>,
    id: String,
    archived: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::set_note_archived(book, &id, archived).map_err(|e| e.to_string())
    })
}

/// Set a note's lock mode (`none`, `unlock_delay`, or `fact_check_gate`).
#[tauri::command]
pub fn set_note_lock(
    state: State<AppState>,
    id: String,
    mode: LockMode,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::set_note_lock(book, &id, mode).map_err(|e| e.to_string())
    })
}

/// Toggle a category's private flag.
#[tauri::command]
pub fn set_category_private(
    state: State<AppState>,
    name: String,
    private: bool,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_category_private(book, &name, private).map_err(|e| e.to_string())
    })
}

/// Mark a note for deletion (starts the deletion-delay window instead of removing it now).
#[tauri::command]
pub fn request_deletion(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::request_deletion(book, &id).map_err(|e| e.to_string())
    })
}

/// Cancel a pending deletion.
#[tauri::command]
pub fn restore_note(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::restore_note(book, &id).map_err(|e| e.to_string())
    })
}

/// Permanently remove every note whose deletion delay elapsed or `vanish_at` passed; returns the
/// purged ids. The shell calls this on startup and from an "empty trash" action.
#[tauri::command]
pub fn purge_expired(state: State<AppState>) -> Result<Vec<String>, String> {
    with_book!(state, book, {
        app::purge_expired_now(book).map_err(|e| e.to_string())
    })
}
