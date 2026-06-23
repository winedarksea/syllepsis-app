//! Global application state threaded through Tauri commands via [`tauri::State`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use syllepsis_core::llm::{select_llm_provider, LlmService};
use syllepsis_core::onnx::{manifest, ModelCache};
use syllepsis_core::storage::Book;

pub const MODEL_CACHE_ENV_VAR: &str = "SYLLEPSIS_MODEL_CACHE";

struct CachedLlmService {
    cache_key: String,
    service: LlmService,
}

/// The single app-level state. The open book is behind a Mutex; `None` means no book
/// is open yet (the user hasn't opened or created one in this session).
pub struct AppState {
    pub book: Mutex<Option<Book>>,
    llm_service: Mutex<Option<CachedLlmService>>,
    pub file_watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            book: Mutex::new(None),
            llm_service: Mutex::new(None),
            file_watcher: Mutex::new(None),
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
            let provider = select_llm_provider(book.models_root(), &book.config.llm)
                .map_err(|error| error.to_string())?;
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
    std::env::var_os(MODEL_CACHE_ENV_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data_dir.join("models"))
}

fn llm_cache_key(book: &Book) -> Result<String, String> {
    let llm_config = serde_json::to_string(&book.config.llm).map_err(|e| e.to_string())?;
    Ok(format!(
        "{}\n{}\n{}\n{}",
        book.root.display(),
        book.models_root()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        local_llm_cache_fingerprint(book),
        llm_config
    ))
}

fn local_llm_cache_fingerprint(book: &Book) -> String {
    if book.config.llm.provider != syllepsis_core::llm::LOCAL_PROVIDER {
        return "local-cache:not-routed".to_string();
    }
    let Some(models_root) = book.models_root() else {
        return "local-cache:no-root".to_string();
    };
    let Some(model_manifest) = manifest::builtin(&book.config.llm.local_model) else {
        return format!("local-cache:unknown:{}", book.config.llm.local_model);
    };
    let cache = ModelCache::new(models_root);
    if cache.is_cached(&model_manifest) {
        format!("local-cache:cached:{}", model_manifest.id)
    } else {
        let missing_files = cache
            .missing_files(&model_manifest)
            .into_iter()
            .map(|file| file.file_name().to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("local-cache:missing:{}:{missing_files}", model_manifest.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn models_root_prefers_explicit_cache_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let app_data_dir = Path::new("/tmp/syllepsis-app-data");
        std::env::set_var(MODEL_CACHE_ENV_VAR, "/tmp/syllepsis-model-cache");

        let root = models_root_from_app_data(app_data_dir);

        std::env::remove_var(MODEL_CACHE_ENV_VAR);
        assert_eq!(root, PathBuf::from("/tmp/syllepsis-model-cache"));
    }

    #[test]
    fn models_root_uses_app_data_when_env_var_is_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        let app_data_dir = Path::new("/tmp/syllepsis-app-data");
        std::env::remove_var(MODEL_CACHE_ENV_VAR);

        let root = models_root_from_app_data(app_data_dir);

        assert_eq!(root, app_data_dir.join("models"));
    }
}
