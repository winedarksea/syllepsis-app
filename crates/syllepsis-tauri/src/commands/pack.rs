//! Commands for knowledge packs (Phase 6, core-concepts.md): export a curated pack to a file, and
//! preview / import an incoming pack file with category mapping and local-modification protection.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use syllepsis_core::app::pack::{
    self as app, ExportSpec, ImportOptions, ImportPreview, ImportReport, LockedNoteHandling,
};
use syllepsis_core::pack::PackManifest;

use crate::commands::book::{folder_name_for_book, track_book_path, BookInfo};
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

/// The export command's result: the manifest for the confirmation toast, plus how many PIN-locked
/// notes were skipped from the selection (`spec.locked_note_handling == Skip`, the default).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPackResult {
    pub manifest: PackManifest,
    pub skipped_locked_notes: usize,
}

/// Export the notes selected by `spec` to a `.synpack.json` file at `path`.
///
/// `spec.locked_note_handling == Decrypt` requires an unlocked PIN-lock session; if the session is
/// locked this returns the literal error string `"pin_required"`, which the frontend maps to the
/// unlock modal rather than a generic error toast (privacy-security.md "PIN-Locked Notes": packs
/// never contain ciphertext, so a locked note is either skipped or decrypted, never bundled as-is).
#[tauri::command]
pub fn export_pack(
    state: State<AppState>,
    spec: ExportSpec,
    path: String,
) -> Result<ExportPackResult, String> {
    let key = state.pin_session.lock().unwrap().key().cloned();
    if spec.locked_note_handling == LockedNoteHandling::Decrypt && key.is_none() {
        return Err("pin_required".to_string());
    }
    with_book!(state, book, {
        let (manifest, skipped_locked_notes) =
            app::export_pack_with_key(book, &spec, Path::new(&path), key.as_ref())
                .map_err(|e| e.to_string())?;
        Ok(ExportPackResult {
            manifest,
            skipped_locked_notes,
        })
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

/// Read only the manifest from a pack file. Used before any book is open.
#[tauri::command]
pub fn read_pack_manifest(path: String) -> Result<PackManifest, String> {
    let pack = app::read_pack(Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(pack.manifest)
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
        let report = app::import_pack(book, &pack, &options).map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_all_stale(book, false);
        Ok(report)
    })
}

/// Load a pack file and create a brand-new book from its contents, then open it.
#[tauri::command]
pub fn import_pack_as_book(
    app: AppHandle,
    state: State<AppState>,
    pack_path: String,
    parent_path: String,
    book_name: String,
) -> Result<BookInfo, String> {
    let pack = app::read_pack(Path::new(&pack_path)).map_err(|e| e.to_string())?;
    let folder_name = folder_name_for_book(&book_name);
    let book_path = Path::new(&parent_path).join(&folder_name);

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let models_root = models_root_from_app_data(&app_data_dir);

    let book = app::import_pack_as_new_book(&book_path, &book_name, &pack)
        .map_err(|e| e.to_string())?
        .with_models_root(models_root);
    let info = BookInfo {
        name: book.metadata.name.clone(),
        path: book_path.to_string_lossy().into(),
        open_warning: None,
    };
    track_book_path(&app, &book_path)?;
    *state.book.lock().unwrap() = Some(book);
    state.pin_session.lock().unwrap().lock();
    state.invalidate_llm_service();
    if let Some(book) = state.book.lock().unwrap().as_ref() {
        let _ = state.local_ai.enqueue_all_stale(book, false);
    }
    Ok(info)
}
