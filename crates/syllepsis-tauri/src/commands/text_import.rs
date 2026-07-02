//! Commands for deterministic long-text import preview and commit.

use tauri::State;

use syllepsis_core::app::text_import::{
    self as app,
    chunks::{self, TextImportLlmChunk},
    keywords::{self, TextImportCategorySuggestion},
    TextImportCommitRequest, TextImportOptions, TextImportPreview, TextImportPreviewItem,
    TextImportReport,
};

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
pub fn read_text_import_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("read text import file: {e}"))
}

#[tauri::command]
pub fn preview_text_import(
    source_text: String,
    options: TextImportOptions,
) -> Result<TextImportPreview, String> {
    Ok(app::preview_text_import(&source_text, &options))
}

/// Deterministic keyword-based category suggestions over the current preview items.
#[tauri::command]
pub fn suggest_text_import_categories(
    items: Vec<TextImportPreviewItem>,
    max: usize,
) -> Result<Vec<TextImportCategorySuggestion>, String> {
    Ok(keywords::suggest_categories(&items, max))
}

/// Pack preview items into LLM-sized chunks (sections stay intact by construction).
#[tauri::command]
pub fn chunk_text_import_for_llm(
    items: Vec<TextImportPreviewItem>,
    max_chars: usize,
) -> Result<Vec<TextImportLlmChunk>, String> {
    Ok(chunks::chunk_items_for_llm(&items, max_chars))
}

#[tauri::command]
pub fn commit_text_import(
    state: State<AppState>,
    request: TextImportCommitRequest,
) -> Result<TextImportReport, String> {
    with_book!(state, book, {
        let report = app::commit_text_import(book, request).map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_all_stale(book, false);
        Ok(report)
    })
}
