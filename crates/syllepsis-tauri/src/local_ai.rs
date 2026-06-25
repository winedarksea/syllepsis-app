//! One prioritized, session-scoped queue for every in-process model call.

mod power;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use syllepsis_core::app;
use syllepsis_core::config::ModelRef;
use syllepsis_core::embeddings::{
    generate_note_sidecar, stale_or_missing_note_ids, try_select_embedder, Embedding,
    EmbeddingProvider,
};
use syllepsis_core::id::NoteId;
use syllepsis_core::llm::{select_llm_provider, LlmService, LlmTask, Proposal};
use syllepsis_core::storage::{Book, NoteStore};

use power::detect_power_source;
pub use power::PowerSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalAiDevicePolicy {
    pub generate_note_embeddings: bool,
    pub pause_note_embeddings_on_battery: bool,
    pub note_embedding_debounce_seconds: u64,
    pub model_idle_unload_seconds: u64,
}

impl Default for LocalAiDevicePolicy {
    fn default() -> Self {
        LocalAiDevicePolicy {
            generate_note_embeddings: !cfg!(mobile),
            pause_note_embeddings_on_battery: true,
            note_embedding_debounce_seconds: 60,
            model_idle_unload_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalAiFailure {
    pub occurred_at: chrono::DateTime<Utc>,
    pub job: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalAiWorkerStatus {
    pub current_job: Option<String>,
    pub pending_llm_jobs: usize,
    pub pending_query_jobs: usize,
    pub pending_note_jobs: usize,
    pub blocked_note_jobs: usize,
    pub note_block_reason: Option<String>,
    pub power_source: PowerSource,
    pub policy: LocalAiDevicePolicy,
    pub recent_failures: Vec<LocalAiFailure>,
}

struct LlmJob {
    book_root: PathBuf,
    models_root: PathBuf,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
    response: mpsc::SyncSender<Result<Proposal, String>>,
}

struct QueryJob {
    book_root: PathBuf,
    models_root: PathBuf,
    query: String,
    response: mpsc::SyncSender<Result<Embedding, String>>,
}

#[derive(Clone)]
struct NoteJob {
    book_root: PathBuf,
    models_root: PathBuf,
    note_id: String,
    due_at: Instant,
}

struct WorkerState {
    llm_jobs: VecDeque<LlmJob>,
    query_jobs: VecDeque<QueryJob>,
    note_jobs: HashMap<String, NoteJob>,
    blocked_note_jobs: HashMap<String, NoteJob>,
    note_block_reason: Option<String>,
    current_job: Option<String>,
    policy: LocalAiDevicePolicy,
    preferences_path: Option<PathBuf>,
    recent_failures: VecDeque<LocalAiFailure>,
}

struct Shared {
    state: Mutex<WorkerState>,
    wake: Condvar,
}

#[derive(Clone)]
pub struct LocalAiWorker {
    shared: Arc<Shared>,
}

impl LocalAiWorker {
    pub fn new() -> LocalAiWorker {
        let shared = Arc::new(Shared {
            state: Mutex::new(WorkerState {
                llm_jobs: VecDeque::new(),
                query_jobs: VecDeque::new(),
                note_jobs: HashMap::new(),
                blocked_note_jobs: HashMap::new(),
                note_block_reason: None,
                current_job: None,
                policy: LocalAiDevicePolicy::default(),
                preferences_path: None,
                recent_failures: VecDeque::new(),
            }),
            wake: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("syllepsis-local-ai".into())
            .spawn(move || worker_loop(thread_shared))
            .expect("start local AI worker");
        LocalAiWorker { shared }
    }

    pub fn configure_preferences_path(&self, path: PathBuf) {
        let policy = std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let mut state = self.shared.state.lock().unwrap();
        state.preferences_path = Some(path);
        state.policy = policy;
        self.shared.wake.notify_all();
    }

    pub fn policy(&self) -> LocalAiDevicePolicy {
        self.shared.state.lock().unwrap().policy.clone()
    }

    pub fn update_policy(
        &self,
        policy: LocalAiDevicePolicy,
    ) -> Result<LocalAiDevicePolicy, String> {
        let path = {
            let mut state = self.shared.state.lock().unwrap();
            state.policy = policy.clone();
            state.preferences_path.clone()
        };
        if let Some(path) = path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&policy).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        self.shared.wake.notify_all();
        Ok(policy)
    }

    pub fn enqueue_note(&self, book: &Book, note_id: String, expedite: bool) -> Result<(), String> {
        let models_root = book
            .models_root()
            .ok_or_else(|| "local model directory unavailable".to_string())?
            .to_path_buf();
        let mut state = self.shared.state.lock().unwrap();
        let delay = if expedite {
            Duration::ZERO
        } else {
            Duration::from_secs(state.policy.note_embedding_debounce_seconds)
        };
        let key = note_job_key(&book.root, &note_id);
        state.note_jobs.insert(
            key.clone(),
            NoteJob {
                book_root: book.root.clone(),
                models_root,
                note_id,
                due_at: Instant::now() + delay,
            },
        );
        state.blocked_note_jobs.remove(&key);
        self.shared.wake.notify_all();
        Ok(())
    }

    pub fn enqueue_note_path(
        &self,
        book_root: PathBuf,
        models_root: PathBuf,
        note_id: String,
        expedite: bool,
    ) {
        let mut state = self.shared.state.lock().unwrap();
        let delay = if expedite {
            Duration::ZERO
        } else {
            Duration::from_secs(state.policy.note_embedding_debounce_seconds)
        };
        let key = note_job_key(&book_root, &note_id);
        state.note_jobs.insert(
            key.clone(),
            NoteJob {
                book_root,
                models_root,
                note_id,
                due_at: Instant::now() + delay,
            },
        );
        state.blocked_note_jobs.remove(&key);
        self.shared.wake.notify_all();
    }

    pub fn enqueue_all_stale(&self, book: &Book, expedite: bool) -> Result<usize, String> {
        let notes = book
            .store
            .read_all_notes()
            .map_err(|error| error.to_string())?;
        let ids = stale_or_missing_note_ids(book, &notes).map_err(|error| error.to_string())?;
        self.shared.state.lock().unwrap().note_block_reason = None;
        for id in &ids {
            self.enqueue_note(book, id.clone(), expedite)?;
        }
        Ok(ids.len())
    }

    pub fn submit_query(&self, book: &Book, query: String) -> Result<Embedding, String> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let models_root = book
            .models_root()
            .ok_or_else(|| "local model directory unavailable".to_string())?
            .to_path_buf();
        self.shared
            .state
            .lock()
            .unwrap()
            .query_jobs
            .push_back(QueryJob {
                book_root: book.root.clone(),
                models_root,
                query,
                response: response_tx,
            });
        self.shared.wake.notify_all();
        response_rx
            .recv()
            .map_err(|_| "local AI worker stopped".to_string())?
    }

    pub fn submit_llm(
        &self,
        book: &Book,
        note_id: String,
        task: LlmTask,
        model_override: Option<ModelRef>,
    ) -> Result<Proposal, String> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let models_root = book
            .models_root()
            .ok_or_else(|| "local model directory unavailable".to_string())?
            .to_path_buf();
        self.shared
            .state
            .lock()
            .unwrap()
            .llm_jobs
            .push_back(LlmJob {
                book_root: book.root.clone(),
                models_root,
                note_id,
                task,
                model_override,
                response: response_tx,
            });
        self.shared.wake.notify_all();
        response_rx
            .recv()
            .map_err(|_| "local AI worker stopped".to_string())?
    }

    pub fn status(&self) -> LocalAiWorkerStatus {
        let power_source = detect_power_source();
        let state = self.shared.state.lock().unwrap();
        let policy_blocked = note_generation_blocked(&state.policy, &power_source);
        let model_blocked = state.note_block_reason.is_some();
        let pending_note_jobs = state.note_jobs.len() + state.blocked_note_jobs.len();
        LocalAiWorkerStatus {
            current_job: state.current_job.clone(),
            pending_llm_jobs: state.llm_jobs.len(),
            pending_query_jobs: state.query_jobs.len(),
            pending_note_jobs,
            blocked_note_jobs: if policy_blocked || model_blocked {
                pending_note_jobs
            } else {
                state.blocked_note_jobs.len()
            },
            note_block_reason: state.note_block_reason.clone().or_else(|| {
                policy_blocked.then(|| note_generation_block_reason(&state.policy, &power_source))
            }),
            power_source,
            policy: state.policy.clone(),
            recent_failures: state.recent_failures.iter().cloned().collect(),
        }
    }
}

impl Default for LocalAiWorker {
    fn default() -> Self {
        Self::new()
    }
}

enum NextJob {
    Llm(LlmJob),
    Query(QueryJob),
    Note(NoteJob),
}

enum RuntimeCache {
    None,
    Embedding {
        key: String,
        provider: Box<dyn EmbeddingProvider>,
    },
    Llm {
        key: String,
        service: Box<LlmService>,
    },
}

fn worker_loop(shared: Arc<Shared>) {
    let mut runtime = RuntimeCache::None;
    let mut last_job_finished = Instant::now();
    loop {
        let job = {
            let mut state = shared.state.lock().unwrap();
            loop {
                let power = detect_power_source();
                if let Some(job) = state.llm_jobs.pop_front() {
                    state.current_job = Some(format!("llm:{}", job.task.as_str()));
                    break NextJob::Llm(job);
                }
                if let Some(job) = state.query_jobs.pop_front() {
                    state.current_job = Some("embedding:search-query".into());
                    break NextJob::Query(job);
                }
                if !note_generation_blocked(&state.policy, &power)
                    && state.note_block_reason.is_none()
                {
                    if let Some((key, job)) = next_ready_note_job(&state.note_jobs) {
                        state.note_jobs.remove(&key);
                        state.current_job = Some(format!("embedding:note:{}", job.note_id));
                        break NextJob::Note(job);
                    }
                }
                let idle = Duration::from_secs(state.policy.model_idle_unload_seconds);
                if !matches!(runtime, RuntimeCache::None) && last_job_finished.elapsed() >= idle {
                    runtime = RuntimeCache::None;
                }
                let wait = next_note_wait(&state.note_jobs).unwrap_or(Duration::from_secs(5));
                let (next_state, _) = shared
                    .wake
                    .wait_timeout(state, wait.min(Duration::from_secs(5)))
                    .unwrap();
                state = next_state;
            }
        };

        let (label, error) = match job {
            NextJob::Llm(job) => {
                let label = format!("llm:{}", job.task.as_str());
                let result = process_llm_job(&mut runtime, &job);
                let error = result.as_ref().err().cloned();
                let _ = job.response.send(result);
                (label, error)
            }
            NextJob::Query(job) => {
                let label = "embedding:search-query".to_string();
                let result = process_query_job(&mut runtime, &job);
                let error = result.as_ref().err().cloned();
                let _ = job.response.send(result);
                (label, error)
            }
            NextJob::Note(job) => {
                let label = format!("embedding:note:{}", job.note_id);
                let result = process_note_job(&mut runtime, &job);
                let error = result.err();
                if error.as_deref().is_some_and(model_unavailable) {
                    let key = note_job_key(&job.book_root, &job.note_id);
                    let mut state = shared.state.lock().unwrap();
                    state.blocked_note_jobs.insert(key, job);
                    state.note_block_reason = error;
                    (label, None)
                } else {
                    (label, error)
                }
            }
        };
        last_job_finished = Instant::now();
        let mut state = shared.state.lock().unwrap();
        state.current_job = None;
        if let Some(message) = error {
            state.recent_failures.push_front(LocalAiFailure {
                occurred_at: Utc::now(),
                job: label,
                message,
            });
            state.recent_failures.truncate(10);
        }
        shared.wake.notify_all();
    }
}

fn process_query_job(runtime: &mut RuntimeCache, job: &QueryJob) -> Result<Embedding, String> {
    let book = open_book(&job.book_root, &job.models_root)?;
    let provider = embedding_provider(runtime, &book)?;
    provider
        .try_embed_query(&job.query)
        .map_err(|error| error.to_string())
}

fn process_note_job(runtime: &mut RuntimeCache, job: &NoteJob) -> Result<(), String> {
    let book = open_book(&job.book_root, &job.models_root)?;
    let note = book
        .store
        .read_note(&NoteId::parse(&job.note_id).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let provider = embedding_provider(runtime, &book)?;
    generate_note_sidecar(&book, provider, &note)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn process_llm_job(runtime: &mut RuntimeCache, job: &LlmJob) -> Result<Proposal, String> {
    let book = open_book(&job.book_root, &job.models_root)?;
    let key = format!(
        "{}\n{}",
        book.root.display(),
        serde_json::to_string(&book.config.llm).map_err(|error| error.to_string())?
    );
    if !matches!(runtime, RuntimeCache::Llm { key: cached, .. } if cached == &key) {
        let provider =
            select_llm_provider(book.models_root(), &book.config.llm).map_err(|e| e.to_string())?;
        *runtime = RuntimeCache::Llm {
            key: key.clone(),
            service: Box::new(LlmService::new(provider, book.config.llm.routing.clone())),
        };
    }
    let RuntimeCache::Llm { service, .. } = runtime else {
        unreachable!()
    };
    app::llm::generate_proposal_with_service(
        &book,
        service.as_ref(),
        &job.note_id,
        job.task,
        job.model_override.clone(),
    )
    .map_err(|error| error.to_string())
}

fn embedding_provider<'a>(
    runtime: &'a mut RuntimeCache,
    book: &Book,
) -> Result<&'a dyn EmbeddingProvider, String> {
    let key = format!(
        "{}\n{}",
        book.root.display(),
        serde_json::to_string(&book.config.embedding).map_err(|error| error.to_string())?
    );
    if !matches!(runtime, RuntimeCache::Embedding { key: cached, .. } if cached == &key) {
        let provider = try_select_embedder(book.models_root(), &book.config.embedding)
            .map_err(|error| error.to_string())?;
        *runtime = RuntimeCache::Embedding {
            key: key.clone(),
            provider,
        };
    }
    match runtime {
        RuntimeCache::Embedding { provider, .. } => Ok(&**provider),
        _ => unreachable!(),
    }
}

fn open_book(root: &Path, models_root: &Path) -> Result<Book, String> {
    Book::open(root)
        .map(|book| book.with_models_root(models_root))
        .map_err(|error| error.to_string())
}

fn next_ready_note_job(jobs: &HashMap<String, NoteJob>) -> Option<(String, NoteJob)> {
    let now = Instant::now();
    jobs.iter()
        .filter(|(_, job)| job.due_at <= now)
        .min_by_key(|(_, job)| job.due_at)
        .map(|(key, job)| (key.clone(), job.clone()))
}

fn next_note_wait(jobs: &HashMap<String, NoteJob>) -> Option<Duration> {
    let now = Instant::now();
    jobs.values()
        .map(|job| job.due_at.saturating_duration_since(now))
        .min()
}

fn note_job_key(book_root: &Path, note_id: &str) -> String {
    format!("{}\n{note_id}", book_root.display())
}

fn note_generation_blocked(policy: &LocalAiDevicePolicy, power: &PowerSource) -> bool {
    !policy.generate_note_embeddings
        || (policy.pause_note_embeddings_on_battery && *power == PowerSource::Battery)
}

fn note_generation_block_reason(policy: &LocalAiDevicePolicy, power: &PowerSource) -> String {
    if !policy.generate_note_embeddings {
        return "Note embedding generation is disabled on this device".into();
    }
    if policy.pause_note_embeddings_on_battery && *power == PowerSource::Battery {
        return "Note embedding generation is paused while on battery".into();
    }
    "Note embedding generation is paused".into()
}

fn model_unavailable(message: &str) -> bool {
    message.contains("not downloaded")
        || message.contains("local model directory unavailable")
        || message.contains("does not include local ONNX")
}
