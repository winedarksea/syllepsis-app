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

/// Toggle a note's `private` **preset**: turns the three independent capabilities (hidden,
/// exclude-from-search, exclude-from-publish) on or off together.
#[tauri::command]
pub fn set_note_private(
    state: State<AppState>,
    id: String,
    private: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::set_note_private(book, &id, private).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Toggle a note's `hidden` flag (out of the main UI / default views / exports).
#[tauri::command]
pub fn set_note_hidden(
    state: State<AppState>,
    id: String,
    hidden: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::set_note_hidden(book, &id, hidden).map_err(|e| e.to_string())?;
        // Hiding changes the default corpus the graph/RAG build over.
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Toggle a note's `exclude_from_search` flag (out of search + RAG retrieval).
#[tauri::command]
pub fn set_note_exclude_from_search(
    state: State<AppState>,
    id: String,
    exclude: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated =
            app::set_note_exclude_from_search(book, &id, exclude).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Toggle a note's `exclude_from_publish` flag (gitignored + withheld from the publish). Publish
/// reads from disk, so this does not invalidate the in-memory graph/RAG corpus.
#[tauri::command]
pub fn set_note_exclude_from_publish(
    state: State<AppState>,
    id: String,
    exclude: bool,
) -> Result<NoteDto, String> {
    with_book!(state, book, {
        app::set_note_exclude_from_publish(book, &id, exclude).map_err(|e| e.to_string())
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
        let updated = app::set_note_archived(book, &id, archived).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(updated)
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

/// Toggle a category's `private` **preset**: turns all three independent capabilities on or off
/// together for the category.
#[tauri::command]
pub fn set_category_private(
    state: State<AppState>,
    name: String,
    private: bool,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_category_private(book, &name, private).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(())
    })
}

/// Toggle a category's `hidden` flag.
#[tauri::command]
pub fn set_category_hidden(
    state: State<AppState>,
    name: String,
    hidden: bool,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_category_hidden(book, &name, hidden).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(())
    })
}

/// Toggle a category's `exclude_from_search` flag.
#[tauri::command]
pub fn set_category_exclude_from_search(
    state: State<AppState>,
    name: String,
    exclude: bool,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_category_exclude_from_search(book, &name, exclude).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(())
    })
}

/// Toggle a category's `exclude_from_publish` flag (publish reads from disk; no corpus invalidation).
#[tauri::command]
pub fn set_category_exclude_from_publish(
    state: State<AppState>,
    name: String,
    exclude: bool,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_category_exclude_from_publish(book, &name, exclude).map_err(|e| e.to_string())
    })
}

/// Mark a note for deletion (starts the deletion-delay window instead of removing it now).
#[tauri::command]
pub fn request_deletion(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::request_deletion(book, &id).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Cancel a pending deletion.
#[tauri::command]
pub fn restore_note(state: State<AppState>, id: String) -> Result<NoteDto, String> {
    with_book!(state, book, {
        let updated = app::restore_note(book, &id).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(updated)
    })
}

/// Permanently remove every note whose deletion delay elapsed or `vanish_at` passed; returns the
/// purged ids. The shell calls this on startup and from an "empty trash" action.
#[tauri::command]
pub fn purge_expired(state: State<AppState>) -> Result<Vec<String>, String> {
    with_book!(state, book, {
        let purged = app::purge_expired_now(book).map_err(|e| e.to_string())?;
        if !purged.is_empty() {
            state.invalidate_graph_corpus();
        }
        Ok(purged)
    })
}

/// Permanently remove every note in the trash immediately, ignoring the configured delay.
#[tauri::command]
pub fn purge_all_trash(state: State<AppState>) -> Result<Vec<String>, String> {
    with_book!(state, book, {
        let purged = app::purge_all_trash(book).map_err(|e| e.to_string())?;
        if !purged.is_empty() {
            state.invalidate_graph_corpus();
        }
        Ok(purged)
    })
}

/// Permanently delete a Picture/Drawing note and its tracked asset immediately.
#[tauri::command]
pub fn delete_image_object_now(state: State<AppState>, id: String) -> Result<(), String> {
    with_book!(state, book, {
        app::delete_image_object_now(book, &id).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(())
    })
}
