//! Commands for knowledge packs (Phase 6, core-concepts.md): export a curated pack to a file, and
//! preview / import an incoming pack file with category mapping and local-modification protection.

use std::path::Path;

use tauri::State;

use syllepsis_core::app::pack::{
    self as app, ExportSpec, ImportOptions, ImportPreview, ImportReport,
};
use syllepsis_core::pack::PackManifest;

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

/// Export the notes selected by `spec` to a `.synpack.json` file at `path`; returns the manifest.
#[tauri::command]
pub fn export_pack(
    state: State<AppState>,
    spec: ExportSpec,
    path: String,
) -> Result<PackManifest, String> {
    with_book!(state, book, {
        app::export_pack(book, &spec, Path::new(&path)).map_err(|e| e.to_string())
    })
}

/// Read a pack file and dry-run it against the open book (per-note status + category suggestions).
#[tauri::command]
pub fn preview_pack(state: State<AppState>, path: String) -> Result<ImportPreview, String> {
    with_book!(state, book, {
        let pack = app::read_pack(Path::new(&path)).map_err(|e| e.to_string())?;
        app::preview_import(book, &pack).map_err(|e| e.to_string())
    })
}

/// Import a pack file into the open book using the user's selection and category mapping.
#[tauri::command]
pub fn import_pack(
    state: State<AppState>,
    path: String,
    options: ImportOptions,
) -> Result<ImportReport, String> {
    with_book!(state, book, {
        let pack = app::read_pack(Path::new(&path)).map_err(|e| e.to_string())?;
        app::import_pack(book, &pack, &options).map_err(|e| e.to_string())
    })
}
