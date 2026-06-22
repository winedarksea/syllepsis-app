//! Commands for opening and creating books, and fetching the version string.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use syllepsis_core::storage::Book;

use crate::state::{models_root_from_app_data, AppState};

/// Lightweight summary of the currently-open book sent to the UI on open/create.
#[derive(Debug, Serialize, Deserialize)]
pub struct BookInfo {
    pub name: String,
    pub path: String,
}

/// Open an existing Syllepsis book at `path`.
#[tauri::command]
pub fn open_book(app: AppHandle, state: State<AppState>, path: String) -> Result<BookInfo, String> {
    let book_path = PathBuf::from(&path);
    let models_root = models_root(&app)?;
    let book = Book::open(&book_path)
        .map(|book| book.with_models_root(models_root))
        .map_err(|e| e.to_string())?;
    let info = BookInfo {
        name: book.metadata.name.clone(),
        path: book_path.display().to_string(),
    };
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    Ok(info)
}

/// Create a new Syllepsis book at `path` with the given `name`.
#[tauri::command]
pub fn create_book(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    name: String,
) -> Result<BookInfo, String> {
    let book_path = PathBuf::from(&path);
    let models_root = models_root(&app)?;
    let book = Book::create(&book_path, &name)
        .map(|book| book.with_models_root(models_root))
        .map_err(|e| e.to_string())?;
    let info = BookInfo {
        name: book.metadata.name.clone(),
        path: book_path.display().to_string(),
    };
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    Ok(info)
}

/// Return the core library version string (used to verify the IPC bridge is alive).
#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(models_root_from_app_data(&app_data_dir))
}
