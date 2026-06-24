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
pub struct TrackedBookInfo {
    pub name: String,
    pub path: String,
    pub available: bool,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookOpenWarningInfo {
    pub missing_reserved_files: Vec<String>,
    pub should_offer_create_here: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrackedBookPaths {
    paths: Vec<String>,
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
    track_book_path(&app, &book_path)?;
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    state.invalidate_graph_corpus();
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
    track_book_path(&app, &book_path)?;
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    state.invalidate_graph_corpus();
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
    track_book_path(&app, &book_path)?;
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    state.invalidate_graph_corpus();
    Ok(info)
}

/// Books this device has successfully opened, created, or imported.
#[tauri::command]
pub fn list_tracked_books(app: AppHandle) -> Result<Vec<TrackedBookInfo>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    list_tracked_books_from_app_data(&app_data_dir)
}

/// Remove a book from the launcher's tracked list without touching files on disk.
#[tauri::command]
pub fn forget_tracked_book(app: AppHandle, path: String) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    forget_tracked_book_from_app_data(&app_data_dir, Path::new(&path))
}

/// Return the core library version string (used to verify the IPC bridge is alive).
#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// App version and build date for the Settings → About panel. The build date is baked in by
/// `build.rs` via the `SYLLEPSIS_BUILD_DATE` env var.
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    pub version: String,
    pub build_date: String,
}

/// Return the app version and the date this binary was built.
#[tauri::command]
pub fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_date: env!("SYLLEPSIS_BUILD_DATE").to_string(),
    }
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

pub(crate) fn track_book_path(app: &AppHandle, path: &Path) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    track_book_path_in_app_data(&app_data_dir, path)
}

fn list_tracked_books_from_app_data(app_data_dir: &Path) -> Result<Vec<TrackedBookInfo>, String> {
    let tracked = load_tracked_book_paths(app_data_dir)?;
    Ok(tracked
        .paths
        .into_iter()
        .map(|path| tracked_book_info(Path::new(&path)))
        .collect())
}

fn forget_tracked_book_from_app_data(app_data_dir: &Path, path: &Path) -> Result<(), String> {
    let mut tracked = load_tracked_book_paths(app_data_dir)?;
    let normalized_path = normalized_book_path(path);
    tracked
        .paths
        .retain(|tracked_path| tracked_path != &normalized_path);
    save_tracked_book_paths(app_data_dir, &tracked)
}

fn track_book_path_in_app_data(app_data_dir: &Path, path: &Path) -> Result<(), String> {
    let mut tracked = load_tracked_book_paths(app_data_dir)?;
    let normalized_path = normalized_book_path(path);
    tracked
        .paths
        .retain(|tracked_path| tracked_path != &normalized_path);
    tracked.paths.insert(0, normalized_path);
    save_tracked_book_paths(app_data_dir, &tracked)
}

fn tracked_book_info(path: &Path) -> TrackedBookInfo {
    let path_string = path.display().to_string();
    if !path.exists() {
        return TrackedBookInfo {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Missing Book")
                .to_string(),
            path: path_string,
            available: false,
            status: Some("not found on disk".to_string()),
        };
    }

    if !path.is_dir() {
        return TrackedBookInfo {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Unavailable Book")
                .to_string(),
            path: path_string,
            available: false,
            status: Some("path is not a folder".to_string()),
        };
    }

    match Book::open(path) {
        Ok(book) => TrackedBookInfo {
            name: book.metadata.name,
            path: path_string,
            available: true,
            status: book
                .open_warning
                .map(|warning| format!("missing {}", warning.missing_reserved_files.join(", "))),
        },
        Err(error) => TrackedBookInfo {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Unavailable Book")
                .to_string(),
            path: path_string,
            available: false,
            status: Some(error.to_string()),
        },
    }
}

fn load_tracked_book_paths(app_data_dir: &Path) -> Result<TrackedBookPaths, String> {
    let path = tracked_books_path(app_data_dir);
    if !path.exists() {
        return Ok(TrackedBookPaths::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read tracked books {}: {e}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("parse tracked books {}: {e}", path.display()))
}

fn save_tracked_book_paths(app_data_dir: &Path, tracked: &TrackedBookPaths) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("create app data dir {}: {e}", app_data_dir.display()))?;
    let path = tracked_books_path(app_data_dir);
    let content = serde_json::to_string_pretty(tracked).map_err(|e| e.to_string())?;
    std::fs::write(&path, content)
        .map_err(|e| format!("write tracked books {}: {e}", path.display()))
}

fn tracked_books_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("tracked-books.json")
}

fn normalized_book_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
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

    #[test]
    fn tracked_book_persistence_deduplicates_paths() {
        let app_data_dir = tempfile::tempdir().unwrap();
        let book_dir = tempfile::tempdir().unwrap();

        track_book_path_in_app_data(app_data_dir.path(), book_dir.path()).unwrap();
        track_book_path_in_app_data(app_data_dir.path(), book_dir.path()).unwrap();

        let tracked = load_tracked_book_paths(app_data_dir.path()).unwrap();
        assert_eq!(tracked.paths.len(), 1);
        assert_eq!(tracked.paths[0], normalized_book_path(book_dir.path()));
    }

    #[test]
    fn list_tracked_books_returns_existing_book_names() {
        let app_data_dir = tempfile::tempdir().unwrap();
        let parent_dir = tempfile::tempdir().unwrap();
        let book_root = parent_dir.path().join("field-book");
        Book::create(&book_root, "Field Book").unwrap();

        track_book_path_in_app_data(app_data_dir.path(), &book_root).unwrap();

        let books = list_tracked_books_from_app_data(app_data_dir.path()).unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].name, "Field Book");
        assert!(books[0].available);
    }

    #[test]
    fn list_tracked_books_marks_missing_paths_unavailable() {
        let app_data_dir = tempfile::tempdir().unwrap();
        let missing_path = app_data_dir.path().join("missing-book");

        track_book_path_in_app_data(app_data_dir.path(), &missing_path).unwrap();

        let books = list_tracked_books_from_app_data(app_data_dir.path()).unwrap();
        assert_eq!(books.len(), 1);
        assert!(!books[0].available);
        assert_eq!(books[0].status.as_deref(), Some("not found on disk"));
    }

    #[test]
    fn forget_tracked_book_removes_only_launcher_entry() {
        let app_data_dir = tempfile::tempdir().unwrap();
        let book_dir = tempfile::tempdir().unwrap();

        track_book_path_in_app_data(app_data_dir.path(), book_dir.path()).unwrap();
        forget_tracked_book_from_app_data(app_data_dir.path(), book_dir.path()).unwrap();

        let tracked = load_tracked_book_paths(app_data_dir.path()).unwrap();
        assert!(tracked.paths.is_empty());
        assert!(book_dir.path().exists());
    }
}
