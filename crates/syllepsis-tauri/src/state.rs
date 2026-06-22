//! Global application state threaded through Tauri commands via [`tauri::State`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use syllepsis_core::llm::{select_llm_provider, LlmService};
use syllepsis_core::storage::Book;

struct CachedLlmService {
    cache_key: String,
    service: LlmService,
}

/// The single app-level state. The open book is behind a Mutex; `None` means no book
/// is open yet (the user hasn't opened or created one in this session).
pub struct AppState {
    pub book: Mutex<Option<Book>>,
    llm_service: Mutex<Option<CachedLlmService>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            book: Mutex::new(None),
            llm_service: Mutex::new(None),
        }
    }

    /// Drop the cached local-model provider after switching books or changing LLM config.
    pub fn invalidate_llm_service(&self) {
        *self.llm_service.lock().unwrap() = None;
    }

    /// Run a closure against the long-lived LLM service for `book`, constructing it on first use or
    /// when the book/config/model cache changes.
    pub fn with_llm_service<T>(
        &self,
        book: &Book,
        f: impl FnOnce(&LlmService) -> Result<T, String>,
    ) -> Result<T, String> {
        let key = llm_cache_key(book)?;
        let mut cached = self.llm_service.lock().unwrap();
        if cached.as_ref().map(|c| c.cache_key.as_str()) != Some(key.as_str()) {
            let provider = select_llm_provider(book.models_root(), &book.config.llm);
            *cached = Some(CachedLlmService {
                cache_key: key,
                service: LlmService::new(provider, book.config.llm.routing.clone()),
            });
        }
        let service = &cached
            .as_ref()
            .expect("cached llm service was just initialized")
            .service;
        f(service)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Machine-local model root shared by all books. This is deliberately outside the synced book.
pub fn models_root_from_app_data(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

fn llm_cache_key(book: &Book) -> Result<String, String> {
    let llm_config = serde_json::to_string(&book.config.llm).map_err(|e| e.to_string())?;
    Ok(format!(
        "{}\n{}\n{}",
        book.root.display(),
        book.models_root()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        llm_config
    ))
}
