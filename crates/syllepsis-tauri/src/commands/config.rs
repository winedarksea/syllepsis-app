//! Read and update the open book's operational config (`_config.yaml`).
//!
//! Each updater replaces a whole sub-config and persists via `book.save_config()`. Because the
//! config types use `#[serde(default)]`, the frontend must send the *complete* sub-config object it
//! read from `get_book_config` — omitted fields fall back to type defaults, not the on-disk value.
//!
//! The `_impl` helpers take `&AppState` (not `tauri::State`) so they can be unit-tested against a
//! real `AppState` holding a temp `Book`.

use tauri::State;

use syllepsis_core::config::{
    CleanupConfig, Config, LlmConfig, PrivacyConfig, SearchConfig, SyncConfig,
};

use crate::state::AppState;

/// Return the open book's full config, or an error if no book is open.
#[tauri::command]
pub fn get_book_config(state: State<AppState>) -> Result<Config, String> {
    get_book_config_impl(&state)
}

#[tauri::command]
pub fn update_privacy_config(
    state: State<AppState>,
    privacy: PrivacyConfig,
) -> Result<Config, String> {
    update_book_config(&state, |config| config.privacy = privacy, false)
}

#[tauri::command]
pub fn update_sync_config(state: State<AppState>, sync: SyncConfig) -> Result<Config, String> {
    update_book_config(&state, |config| config.sync = sync, false)
}

#[tauri::command]
pub fn update_search_config(
    state: State<AppState>,
    search: SearchConfig,
) -> Result<Config, String> {
    update_book_config(&state, |config| config.search = search, false)
}

#[tauri::command]
pub fn update_cleanup_config(
    state: State<AppState>,
    cleanup: CleanupConfig,
) -> Result<Config, String> {
    update_book_config(&state, |config| config.cleanup = cleanup, false)
}

#[tauri::command]
pub fn update_llm_config(state: State<AppState>, llm: LlmConfig) -> Result<Config, String> {
    // Changing LLM config invalidates the cached provider so the next call rebuilds from it.
    update_book_config(&state, |config| config.llm = llm, true)
}

fn get_book_config_impl(state: &AppState) -> Result<Config, String> {
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    Ok(book.config.clone())
}

/// Apply `mutate` to the open book's config, persist it, and return the saved config. When
/// `invalidate_llm` is true the cached LLM service is dropped so it rebuilds from the new config.
fn update_book_config(
    state: &AppState,
    mutate: impl FnOnce(&mut Config),
    invalidate_llm: bool,
) -> Result<Config, String> {
    let updated = {
        let mut guard = state.book.lock().unwrap();
        let book = guard
            .as_mut()
            .ok_or_else(|| "no book is open".to_string())?;
        mutate(&mut book.config);
        book.save_config().map_err(|e| e.to_string())?;
        book.config.clone()
    };
    if invalidate_llm {
        state.invalidate_llm_service();
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syllepsis_core::storage::Book;

    fn state_with_book(root: &std::path::Path) -> AppState {
        let book = Book::create(root, "Test Book").unwrap();
        let state = AppState::new();
        *state.book.lock().unwrap() = Some(book);
        state
    }

    #[test]
    fn get_book_config_errors_without_open_book() {
        let state = AppState::new();
        let error = get_book_config_impl(&state).unwrap_err();
        assert_eq!(error, "no book is open");
    }

    #[test]
    fn update_privacy_config_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        let state = state_with_book(&root);

        let mut privacy = get_book_config_impl(&state).unwrap().privacy;
        privacy.unlock_delay_hours = 72;
        let returned =
            update_book_config(&state, |config| config.privacy = privacy, false).unwrap();
        assert_eq!(returned.privacy.unlock_delay_hours, 72);

        // Reopen from disk to confirm `save_config` wrote the change.
        let reopened = Book::open(&root).unwrap();
        assert_eq!(reopened.config.privacy.unlock_delay_hours, 72);
    }

    #[test]
    fn update_llm_config_persists_provider_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        let state = state_with_book(&root);

        let mut llm = get_book_config_impl(&state).unwrap().llm;
        llm.provider = "anthropic".to_string();
        llm.max_new_tokens = 1024;
        let returned = update_book_config(&state, |config| config.llm = llm, true).unwrap();
        assert_eq!(returned.llm.provider, "anthropic");
        assert_eq!(returned.llm.max_new_tokens, 1024);

        let reopened = Book::open(&root).unwrap();
        assert_eq!(reopened.config.llm.provider, "anthropic");
    }
}
