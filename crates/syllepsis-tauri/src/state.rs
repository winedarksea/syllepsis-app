//! Global application state threaded through Tauri commands via [`tauri::State`].

use crate::local_ai::LocalAiWorker;
use crate::server::ServerHandle;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use syllepsis_core::app::llm::QueuedLlmJobResult;
use syllepsis_core::graph_analysis::{
    current_corpus_fingerprint, GraphAnalysisRequest, GraphAnalysisResult, SemanticGraphCorpus,
};
use syllepsis_core::llm::prompts::LlmTaskOptions;
use syllepsis_core::storage::Book;

pub const MODEL_CACHE_ENV_VAR: &str = "SYLLEPSIS_MODEL_CACHE";

struct CachedGraphCorpus {
    fingerprint: String,
    corpus: Arc<SemanticGraphCorpus>,
}

pub struct QueuedLlmJobRecord {
    pub result: QueuedLlmJobResult,
    pub options: LlmTaskOptions,
    pub dismissed: bool,
}

pub struct CachedCloudLlmModels {
    pub model_ids: Vec<String>,
    pub fetched_at: SystemTime,
}

pub struct CachedCloudLlmCredentials {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

pub struct CachedCloudSyncCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub access_token: Option<String>,
    pub access_token_expires_at: Option<SystemTime>,
    pub refresh_token: Option<String>,
}

/// Transient OAuth handshake state held only in memory for the seconds-long connect flow, so the
/// PKCE verifier and CSRF state never create their own keychain items.
#[derive(Clone)]
pub struct PendingOAuth {
    pub state: String,
    pub verifier: String,
}

/// The single app-level state. The open book is behind a Mutex; `None` means no book
/// is open yet (the user hasn't opened or created one in this session).
pub struct AppState {
    pub book: Mutex<Option<Book>>,
    graph_corpus: Mutex<Option<CachedGraphCorpus>>,
    pub file_watcher: Mutex<Option<notify::RecommendedWatcher>>,
    pub local_ai: LocalAiWorker,
    pub llm_jobs: Arc<Mutex<HashMap<String, QueuedLlmJobRecord>>>,
    pub cloud_llm_models: Arc<Mutex<HashMap<String, CachedCloudLlmModels>>>,
    pub cloud_llm_credentials: Arc<Mutex<HashMap<String, CachedCloudLlmCredentials>>>,
    pub cloud_sync_credentials: Arc<Mutex<HashMap<String, CachedCloudSyncCredentials>>>,
    /// Serializes the single keychain "secrets vault" read-modify-write across both the sync and
    /// cloud-LLM subsystems, which now share one item. Held only around vault access, never across
    /// network calls.
    pub secrets_lock: Arc<Mutex<()>>,
    /// In-flight OAuth handshakes keyed by provider; replaces the transient keychain items that the
    /// connect flow used to write for the CSRF state and PKCE verifier.
    pub pending_oauth: Arc<Mutex<HashMap<String, PendingOAuth>>>,
    /// Serializes cloud sync passes. Separate from `book` so no UI command contends on it;
    /// `try_lock()` gives "only one sync at a time, coalesce overlaps" for free.
    pub sync_lock: Arc<Mutex<()>>,
    /// Debounce counter for auto-sync triggered by `note_editing_finished`. Each call increments
    /// this; the spawned task checks it after a delay and skips if a newer call has since arrived,
    /// so rapid note navigation collapses into a single trailing sync.
    pub sync_debounce_gen: Arc<AtomicU64>,
    /// Running search API server instance, if enabled. `None` when the API is off.
    pub search_api_server: Mutex<Option<ServerHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            book: Mutex::new(None),
            graph_corpus: Mutex::new(None),
            file_watcher: Mutex::new(None),
            local_ai: LocalAiWorker::new(),
            llm_jobs: Arc::new(Mutex::new(HashMap::new())),
            cloud_llm_models: Arc::new(Mutex::new(HashMap::new())),
            cloud_llm_credentials: Arc::new(Mutex::new(HashMap::new())),
            cloud_sync_credentials: Arc::new(Mutex::new(HashMap::new())),
            secrets_lock: Arc::new(Mutex::new(())),
            pending_oauth: Arc::new(Mutex::new(HashMap::new())),
            sync_lock: Arc::new(Mutex::new(())),
            sync_debounce_gen: Arc::new(AtomicU64::new(0)),
            search_api_server: Mutex::new(None),
        }
    }

    /// Compatibility hook for command paths that previously owned a separate LLM cache. The
    /// serial local-AI worker now keys and switches its own runtime.
    pub fn invalidate_llm_service(&self) {}

    pub fn invalidate_graph_corpus(&self) {
        *self.graph_corpus.lock().unwrap() = None;
    }

    pub fn analyze_graph(
        &self,
        request: &GraphAnalysisRequest,
    ) -> Result<GraphAnalysisResult, String> {
        let corpus = {
            let book_guard = self.book.lock().unwrap();
            let book = book_guard
                .as_ref()
                .ok_or_else(|| "no book is open".to_string())?;
            let fingerprint =
                current_corpus_fingerprint(book).map_err(|error| error.to_string())?;
            let mut cached = self.graph_corpus.lock().unwrap();
            if cached.as_ref().map(|entry| entry.fingerprint.as_str()) != Some(fingerprint.as_str())
            {
                let corpus =
                    Arc::new(SemanticGraphCorpus::build(book).map_err(|error| error.to_string())?);
                *cached = Some(CachedGraphCorpus {
                    fingerprint,
                    corpus,
                });
            }
            Arc::clone(
                &cached
                    .as_ref()
                    .expect("graph corpus was initialized")
                    .corpus,
            )
        };
        corpus.analyze(request).map_err(|error| error.to_string())
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
