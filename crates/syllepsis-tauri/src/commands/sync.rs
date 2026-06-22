//! Commands for Phase 4 sync: running a sync pass, reading sync status, and listing the sync
//! targets the app advertises.

use tauri::State;

use syllepsis_core::app::sync as app;
use syllepsis_core::app::sync::SyncStatusDto;
use syllepsis_core::sync::{SyncProviderDescriptor, SyncReport};

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

/// Run one sync pass against a local/mounted folder (a cloud-drive mount or plain directory).
#[tauri::command]
pub fn sync_to_folder(state: State<AppState>, remote_path: String) -> Result<SyncReport, String> {
    with_book!(state, book, {
        app::sync_to_local_folder(book, &remote_path).map_err(|e| e.to_string())
    })
}

/// This book's sync configuration and this device's actor identity.
#[tauri::command]
pub fn sync_status(state: State<AppState>) -> Result<SyncStatusDto, String> {
    with_book!(state, book, {
        app::sync_status(book).map_err(|e| e.to_string())
    })
}

/// The sync targets the app knows how to offer (for the settings UI). No open book required.
#[tauri::command]
pub fn sync_provider_descriptors() -> Vec<SyncProviderDescriptor> {
    app::provider_descriptors()
}
