//! Commands for opening and creating books, and fetching the version string.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use syllepsis_core::storage::{Book, BookMetadata};

use crate::state::{models_root_from_app_data, AppState};

/// Lightweight summary of the currently-open book sent to the UI on open/create.
#[derive(Debug, Serialize, Deserialize)]
pub struct BookInfo {
    pub name: String,
    pub path: String,
    pub open_warning: Option<BookOpenWarningInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookOpenWarningInfo {
    pub missing_reserved_files: Vec<String>,
    pub should_offer_create_here: bool,
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
        open_warning: book
            .open_warning
            .as_ref()
            .map(|warning| BookOpenWarningInfo {
                missing_reserved_files: warning.missing_reserved_files.clone(),
                should_offer_create_here: warning.should_offer_create_here(),
            }),
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
    language: Option<String>,
    location: Option<String>,
) -> Result<BookInfo, String> {
    let book_path = PathBuf::from(&path);
    let models_root = models_root(&app)?;
    let metadata = metadata_from_create_details(name, language, location)?;
    let book = Book::create_with_metadata(&book_path, metadata)
        .map(|book| book.with_models_root(models_root))
        .map_err(|e| e.to_string())?;
    let info = book_info(&book_path, &book);
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    Ok(info)
}

/// Create a new Syllepsis book as a subfolder of `parent_path`.
#[tauri::command]
pub fn create_book_in_parent(
    app: AppHandle,
    state: State<AppState>,
    parent_path: String,
    name: String,
    language: Option<String>,
    location: Option<String>,
) -> Result<BookInfo, String> {
    let parent_path = PathBuf::from(&parent_path);
    let metadata = metadata_from_create_details(name, language, location)?;
    let book_path = parent_path.join(folder_name_for_book(&metadata.name));
    let models_root = models_root(&app)?;
    let book = Book::create_with_metadata(&book_path, metadata)
        .map(|book| book.with_models_root(models_root))
        .map_err(|e| e.to_string())?;
    let info = book_info(&book_path, &book);
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    Ok(info)
}

/// Return the core library version string (used to verify the IPC bridge is alive).
#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn book_info(book_path: &Path, book: &Book) -> BookInfo {
    BookInfo {
        name: book.metadata.name.clone(),
        path: book_path.display().to_string(),
        open_warning: book
            .open_warning
            .as_ref()
            .map(|warning| BookOpenWarningInfo {
                missing_reserved_files: warning.missing_reserved_files.clone(),
                should_offer_create_here: warning.should_offer_create_here(),
            }),
    }
}

fn metadata_from_create_details(
    name: String,
    language: Option<String>,
    location: Option<String>,
) -> Result<BookMetadata, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("book name is required".to_string());
    }

    let mut metadata = BookMetadata::new(name);
    if let Some(language) = trimmed_non_empty(language) {
        metadata.language = language;
    }
    metadata.location = trimmed_non_empty(location);
    Ok(metadata)
}

fn trimmed_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(crate) fn folder_name_for_book(name: &str) -> String {
    let mut folder = String::new();
    let mut previous_was_separator = false;

    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            folder.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            folder.push('-');
            previous_was_separator = true;
        }
    }

    let folder = folder.trim_matches('-');
    if folder.is_empty() {
        "untitled-book".to_string()
    } else {
        folder.to_string()
    }
}

fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(models_root_from_app_data(&app_data_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_for_book_is_filesystem_friendly() {
        assert_eq!(folder_name_for_book("My Field Notes"), "my-field-notes");
        assert_eq!(folder_name_for_book("  Notes: 2026! "), "notes-2026");
        assert_eq!(folder_name_for_book("..."), "untitled-book");
    }

    #[test]
    fn metadata_from_create_details_trims_optional_fields() {
        let metadata = metadata_from_create_details(
            " Field Notes ".to_string(),
            Some(" es ".to_string()),
            Some(" Chicago ".to_string()),
        )
        .unwrap();

        assert_eq!(metadata.name, "Field Notes");
        assert_eq!(metadata.language, "es");
        assert_eq!(metadata.location.as_deref(), Some("Chicago"));
    }
}
