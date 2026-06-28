//! Sync commands: mounted-folder sync, git snapshots, file-watch observability, and managed cloud
//! patch-log sync.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use syllepsis_core::app::git as git_app;
use syllepsis_core::app::sync as app;
use syllepsis_core::app::sync::SyncStatusDto;
use syllepsis_core::id::NoteId;
use syllepsis_core::markdown::split_frontmatter;
use syllepsis_core::storage::{layout, Book, BookMetadata, NoteStore};
use syllepsis_core::sync::{
    build_remote_entries, content_revision, fragment_path, is_cloud_index_path,
    latest_note_activity, list_activity, prune_activity, summarize_activity, CloudIndex,
    CloudIndexFragment, IndexEntry, ListedRemoteFile, ManagedCloudSyncEngine, ManagedObjectEntry,
    ManagedObjectStore, NoteSyncActivity, RemoteEntry, RemoteRevision, SyncActivityEvent,
    SyncActivitySummary, SyncEngine, SyncProvider, SyncProviderDescriptor, SyncReport,
};

use crate::state::{AppState, CachedCloudSyncCredentials};

const SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync";
const DEVELOPMENT_SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync.dev";
const ACCESS_TOKEN_FIELD: &str = "access-token";
const REFRESH_TOKEN_FIELD: &str = "refresh-token";
const OAUTH_STATE_FIELD: &str = "oauth-state";
const CODE_VERIFIER_FIELD: &str = "code-verifier";
const OAUTH_CALLBACK_PATH: &str = "/oauth-callback";
const OAUTH_CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const OAUTH_CLIENT_IDS_LOCAL_FILE: &str = "oauth-client-ids.local.json";
const GOOGLE_DRIVE_CLIENT_SECRET_ENV_VAR: &str = "SYLLEPSIS_GOOGLE_DRIVE_CLIENT_SECRET";
const ACTIVITY_RETENTION_DAYS: i64 = 90;
const WATCH_ACTIVITY_DEBOUNCE: Duration = Duration::from_millis(750);
const MANAGED_CLOUD_AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(15 * 60);
const MANAGED_CLOUD_STATE_FILE_PREFIX: &str = "managed-cloud-";
const MANAGED_CLOUD_STATE_FILE_SUFFIX: &str = ".json";
const CLOUD_SYNC_CONNECTION_MARKERS_FILE: &str = "cloud-sync-connected-providers.json";
const ACTIVE_CLOUD_PROVIDER_FILE: &str = "cloud-provider.json";
const ACCESS_TOKEN_REFRESH_SAFETY_WINDOW: Duration = Duration::from_secs(5 * 60);
const HUMAN_READABLE_CLOUD_ROOT: &str = "Syllepsis/";
const HUMAN_READABLE_BOOK_META_SUFFIX: &str = "_book.md";
const CLOUD_BOOK_LAYOUT_HUMAN_READABLE: &str = "human_readable";
const CLOUD_BOOK_LAYOUT_LEGACY_MANAGED: &str = "legacy_managed";

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// Run one sync pass against a local/mounted folder (a cloud-drive mount or plain directory).
#[tauri::command]
pub fn sync_to_folder(state: State<AppState>, remote_path: String) -> Result<SyncReport, String> {
    with_book!(state, book, {
        let report = app::sync_to_local_folder(book, &remote_path).map_err(|e| e.to_string())?;
        book.store.refresh().map_err(|e| e.to_string())?;
        let _ = state.local_ai.enqueue_all_stale(book, false);
        state.invalidate_graph_corpus();
        Ok(report)
    })
}

/// This book's sync configuration and this device's actor identity.
#[tauri::command]
pub fn sync_status(state: State<AppState>) -> Result<SyncStatusDto, String> {
    with_book!(state, book, {
        app::sync_status(book).map_err(|e| e.to_string())
    })
}

/// The sync targets the app knows how to offer (for the settings UI). No open book required.
#[tauri::command]
pub fn sync_provider_descriptors() -> Vec<SyncProviderDescriptor> {
    app::provider_descriptors()
}

#[tauri::command]
pub fn git_status(state: State<AppState>) -> Result<git_app::GitStatusDto, String> {
    with_book!(state, book, {
        git_app::git_status(book).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn git_init(state: State<AppState>) -> Result<git_app::GitCommandReport, String> {
    with_book!(state, book, {
        git_app::git_init(book).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn git_stage_commit(
    state: State<AppState>,
    selected_paths: Vec<String>,
    message: String,
) -> Result<git_app::GitCommandReport, String> {
    with_book!(state, book, {
        git_app::git_stage_commit(book, &selected_paths, &message).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn git_push(state: State<AppState>) -> Result<git_app::GitCommandReport, String> {
    with_book!(state, book, {
        git_app::git_push(book).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn git_pull(state: State<AppState>) -> Result<git_app::GitCommandReport, String> {
    with_book!(state, book, {
        git_app::git_pull(book).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn start_file_watch(state: State<AppState>) -> Result<(), String> {
    let (root, models_root) = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        (
            book.root.clone(),
            book.models_root()
                .ok_or_else(|| "local model directory unavailable".to_string())?
                .to_path_buf(),
        )
    };
    let watch_root = root.clone();
    let local_ai = state.local_ai.clone();
    let recent_watch_activity = Arc::new(Mutex::new(HashMap::<String, Instant>::new()));
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let _ = syllepsis_core::sync::append_activity(
                    &watch_root,
                    &SyncActivityEvent::new(
                        "file_watch",
                        "error",
                        None,
                        format!("watch error: {error}"),
                    ),
                );
                return;
            }
        };
        for path in event.paths {
            let Some(rel) = watch_activity_path(&watch_root, &path) else {
                continue;
            };
            if should_debounce_watch_activity(&recent_watch_activity, &rel, Instant::now()) {
                continue;
            }
            let kind = watch_activity_kind(&rel);
            if let Some(file_name) = path.file_stem().and_then(|name| name.to_str()) {
                if let Ok(note_id) = NoteId::parse(file_name) {
                    local_ai.enqueue_note_path(
                        watch_root.clone(),
                        models_root.clone(),
                        note_id.to_string(),
                        false,
                    );
                }
            }
            let detail = if kind == "conflict_detected" {
                "conflict copy detected"
            } else {
                "external save detected"
            };
            let _ = syllepsis_core::sync::append_activity(
                &watch_root,
                &SyncActivityEvent::new("file_watch", kind, Some(rel), detail),
            );
        }
        let _ = prune_activity(&watch_root, ACTIVITY_RETENTION_DAYS);
    })
    .map_err(|e| format!("start file watcher: {e}"))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| format!("watch book folder: {e}"))?;
    *state.file_watcher.lock().unwrap() = Some(watcher);
    Ok(())
}

#[tauri::command]
pub fn stop_file_watch(state: State<AppState>) -> Result<(), String> {
    *state.file_watcher.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn sync_activity(state: State<AppState>) -> Result<Vec<SyncActivityEvent>, String> {
    with_book!(state, book, {
        prune_activity(&book.root, ACTIVITY_RETENTION_DAYS).map_err(|e| e.to_string())?;
        list_activity(&book.root).map_err(|e| e.to_string())
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalGitSummary {
    pub available: bool,
    pub is_repository: bool,
    pub branch: Option<String>,
    pub changed_file_count: usize,
    pub commit_safe_note_change_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCloudSummary {
    pub provider_count: usize,
    pub connected_provider_count: usize,
    pub connected_provider_names: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCrdtSummary {
    pub backend: String,
    pub sync_enabled: bool,
    pub note_count: usize,
    pub sidecar_count: usize,
    pub loro_sidecar_coverage_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalActivitySummary {
    pub activity: SyncActivitySummary,
    pub git: OperationalGitSummary,
    pub cloud: OperationalCloudSummary,
    pub crdt: OperationalCrdtSummary,
}

#[tauri::command]
pub fn operational_activity_summary(
    app: AppHandle,
    state: State<AppState>,
) -> Result<OperationalActivitySummary, String> {
    with_book!(state, book, {
        prune_activity(&book.root, ACTIVITY_RETENTION_DAYS).map_err(|e| e.to_string())?;
        let events = list_activity(&book.root).map_err(|e| e.to_string())?;
        let activity = summarize_activity(&events, Utc::now());
        let git = operational_git_summary(book);
        let cloud = operational_cloud_summary(&app);
        let crdt = operational_crdt_summary(book).map_err(|e| e.to_string())?;
        Ok(OperationalActivitySummary {
            activity,
            git,
            cloud,
            crdt,
        })
    })
}

#[tauri::command]
pub fn note_sync_activity(
    state: State<AppState>,
    note_id: String,
) -> Result<Option<NoteSyncActivity>, String> {
    with_book!(state, book, {
        let note_id = NoteId::parse(&note_id).map_err(|e| e.to_string())?;
        prune_activity(&book.root, ACTIVITY_RETENTION_DAYS).map_err(|e| e.to_string())?;
        let events = list_activity(&book.root).map_err(|e| e.to_string())?;
        Ok(latest_note_activity(&events, &note_id, Utc::now()))
    })
}

fn operational_git_summary(book: &Book) -> OperationalGitSummary {
    match git_app::git_status(book) {
        Ok(status) => OperationalGitSummary {
            available: status.available,
            is_repository: status.is_repository,
            branch: status.branch,
            changed_file_count: status.changed_files.len(),
            commit_safe_note_change_count: status
                .changed_files
                .iter()
                .filter(|file| file.stage_by_default)
                .count(),
            error: status.error,
        },
        Err(error) => OperationalGitSummary {
            available: false,
            is_repository: false,
            branch: None,
            changed_file_count: 0,
            commit_safe_note_change_count: 0,
            error: Some(error.to_string()),
        },
    }
}

fn operational_cloud_summary(app: &AppHandle) -> OperationalCloudSummary {
    match load_cloud_sync_connection_markers(app) {
        Ok(connected_provider_ids) => OperationalCloudSummary {
            provider_count: cloud_descriptors().len(),
            connected_provider_count: connected_provider_ids.len(),
            connected_provider_names: cloud_descriptors()
                .into_iter()
                .filter(|descriptor| connected_provider_ids.contains(&descriptor.provider))
                .map(|descriptor| descriptor.display_name)
                .collect(),
            error: None,
        },
        Err(error) => OperationalCloudSummary {
            provider_count: cloud_descriptors().len(),
            connected_provider_count: 0,
            connected_provider_names: Vec::new(),
            error: Some(error),
        },
    }
}

fn operational_crdt_summary(
    book: &Book,
) -> syllepsis_core::error::CoreResult<OperationalCrdtSummary> {
    let note_count = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| note.metadata.lifecycle.marked_for_deletion_at.is_none())
        .count();
    let sidecar_count = std::fs::read_dir(layout::crdt_dir(&book.root))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some(layout::CRDT_EXTENSION)
        })
        .count();
    let loro_sidecar_coverage_percent = if note_count == 0 {
        100
    } else {
        ((sidecar_count.min(note_count) * 100) / note_count) as u8
    };
    Ok(OperationalCrdtSummary {
        backend: book.config.sync.crdt_backend.clone(),
        sync_enabled: book.config.sync.enabled,
        note_count,
        sidecar_count,
        loro_sidecar_coverage_percent,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncProviderDescriptor {
    pub provider: String,
    pub display_name: String,
    pub auth_url_base: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncProviderStatus {
    pub provider: String,
    pub display_name: String,
    pub connected: bool,
    pub requires_loro: bool,
    pub active_for_current_book: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncConnectStart {
    pub provider: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudBookSummary {
    pub book_id: String,
    pub name: String,
    pub updated_at: String,
    pub remote_root: String,
    pub layout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteBookCloudCleanupOutcome {
    pub provider: String,
    pub attempted: bool,
    pub connected: bool,
    pub deleted_object_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteCurrentBookReport {
    pub book_name: String,
    pub book_path: String,
    pub cloud_cleanup: Vec<DeleteBookCloudCleanupOutcome>,
}

#[tauri::command]
pub fn cloud_sync_provider_descriptors() -> Vec<CloudSyncProviderDescriptor> {
    cloud_descriptors()
}

#[tauri::command]
pub fn cloud_sync_provider_statuses(
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<CloudSyncProviderStatus>, String> {
    let connected_provider_ids = load_cloud_sync_connection_markers(&app)?;
    let active_provider = active_cloud_provider_for_current_book(&state)?;
    cloud_descriptors()
        .into_iter()
        .map(|descriptor| {
            Ok(CloudSyncProviderStatus {
                connected: connected_provider_ids.contains(&descriptor.provider),
                active_for_current_book: active_provider.as_deref() == Some(&descriptor.provider),
                requires_loro: true,
                provider: descriptor.provider,
                display_name: descriptor.display_name,
            })
        })
        .collect()
}

#[tauri::command]
pub fn connect_cloud_sync_provider(
    app: tauri::AppHandle,
    provider: String,
) -> Result<CloudSyncConnectStart, String> {
    descriptor_for(&provider)?;
    let oauth_client_config = oauth_client_config(provider.as_str())?;
    let listener = TcpListener::bind(("127.0.0.1", oauth_client_config.callback_port))
        .map_err(|error| format!("start OAuth callback listener: {error}"))?;
    let callback_address = listener
        .local_addr()
        .map_err(|error| format!("read OAuth callback address: {error}"))?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}{OAUTH_CALLBACK_PATH}",
        callback_address.port()
    );
    let state = ulid::Ulid::new().to_string();
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let auth_url = oauth_url(&provider, &state, &challenge, &redirect_uri)?;
    let mut store = KeyringSyncCredentialStore;
    store.set(&account(&provider, OAUTH_STATE_FIELD), &state)?;
    store.set(&account(&provider, CODE_VERIFIER_FIELD), &verifier)?;

    let callback_provider = provider.clone();
    let callback_redirect_uri = redirect_uri.clone();
    thread::spawn(move || {
        let result = receive_oauth_callback(listener).and_then(|callback_url| {
            complete_cloud_sync_oauth_callback(
                &callback_provider,
                &callback_url,
                &callback_redirect_uri,
            )
            .and_then(|completion| {
                mark_cloud_sync_provider_connected(&app, &completion.status.provider, true)?;
                let state = app.state::<AppState>();
                cache_sync_credentials(
                    &state,
                    &completion.status.provider,
                    &completion.credentials,
                );
                Ok(completion.status)
            })
        });
        match result {
            Ok(status) => {
                let _ = app.emit("cloud-sync://oauth-completed", status);
            }
            Err(error) => {
                let _ = app.emit("cloud-sync://oauth-failed", error);
            }
        }
    });

    Ok(CloudSyncConnectStart {
        auth_url,
        redirect_uri,
        provider,
        state,
    })
}

fn complete_cloud_sync_oauth_callback(
    provider: &str,
    callback_url: &str,
    redirect_uri: &str,
) -> Result<CloudSyncOAuthCompletion, String> {
    let params = parse_query_params(callback_url);
    let mut store = KeyringSyncCredentialStore;
    let descriptor = descriptor_for(provider)?;

    let expected_state = store
        .get(&account(provider, OAUTH_STATE_FIELD))?
        .ok_or_else(|| "no pending OAuth request for this provider".to_string())?;
    let callback_state = params
        .get("state")
        .ok_or_else(|| "OAuth callback did not include state".to_string())?;
    if callback_state != &expected_state {
        return Err("OAuth callback state did not match the pending request".to_string());
    }
    store.delete(&account(provider, OAUTH_STATE_FIELD))?;

    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or("authorization was not completed");
        return Err(format!("{error}: {description}"));
    }

    let credentials = if let Some(token) =
        params.get("refresh_token").or_else(|| params.get("token"))
    {
        // Some providers (or manual testing) may deliver a token directly.
        store.set(&account(provider, REFRESH_TOKEN_FIELD), token)?;
        cloud_credentials_for_tokens(provider, None, None, Some(token.clone()))?
    } else if let Some(code) = params.get("code") {
        // Standard authorization-code + PKCE flow: exchange the code for tokens.
        let verifier = store
            .get(&account(provider, CODE_VERIFIER_FIELD))?
            .ok_or_else(|| "no PKCE code verifier found; restart the connect flow".to_string())?;
        store.delete(&account(provider, CODE_VERIFIER_FIELD))?;
        let credentials = exchange_code_for_tokens(provider, code, &verifier, redirect_uri)?;
        if let Some(access) = credentials.access_token.as_ref() {
            store.set(&account(provider, ACCESS_TOKEN_FIELD), access)?;
        }
        if let Some(refresh) = credentials.refresh_token.as_ref() {
            store.set(&account(provider, REFRESH_TOKEN_FIELD), refresh)?;
        }
        credentials
    } else {
        return Err("OAuth callback did not include a token or code".to_string());
    };
    Ok(CloudSyncOAuthCompletion {
        status: CloudSyncProviderStatus {
            provider: descriptor.provider,
            display_name: descriptor.display_name,
            connected: true,
            requires_loro: true,
            active_for_current_book: false,
        },
        credentials,
    })
}

#[tauri::command]
pub fn disconnect_cloud_sync_provider(
    state: State<AppState>,
    app: AppHandle,
    provider: String,
) -> Result<CloudSyncProviderStatus, String> {
    let descriptor = descriptor_for(&provider)?;
    let mut store = KeyringSyncCredentialStore;
    store.delete(&account(&provider, ACCESS_TOKEN_FIELD))?;
    store.delete(&account(&provider, REFRESH_TOKEN_FIELD))?;
    store.delete(&account(&provider, OAUTH_STATE_FIELD))?;
    store.delete(&account(&provider, CODE_VERIFIER_FIELD))?;
    remove_cached_sync_credentials(&state, &provider);
    mark_cloud_sync_provider_connected(&app, &provider, false)?;
    clear_active_cloud_provider_if_matches_current_book(&state, &provider)?;
    Ok(CloudSyncProviderStatus {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        connected: false,
        requires_loro: true,
        active_for_current_book: false,
    })
}

#[tauri::command]
pub fn activate_cloud_sync_provider(
    app: AppHandle,
    state: State<AppState>,
    provider: String,
) -> Result<CloudSyncProviderStatus, String> {
    let descriptor = descriptor_for(&provider)?;
    let connected_provider_ids = load_cloud_sync_connection_markers(&app)?;
    if !connected_provider_ids.contains(&provider) {
        return Err(format!(
            "{} is not connected on this device",
            descriptor.display_name
        ));
    }
    set_active_cloud_provider_for_current_book(&state, &provider)?;
    Ok(CloudSyncProviderStatus {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        connected: true,
        requires_loro: true,
        active_for_current_book: true,
    })
}

#[tauri::command]
pub fn list_cloud_books(
    state: State<AppState>,
    provider: String,
) -> Result<Vec<CloudBookSummary>, String> {
    let store = opendal_store_for(&state, &provider)?;
    list_cloud_books_from_store(&store)
}

fn list_cloud_books_from_store(
    store: &OpenDalManagedObjectStore,
) -> Result<Vec<CloudBookSummary>, String> {
    let mut summaries = human_readable_cloud_book_summaries(store)?;
    summaries.extend(legacy_managed_cloud_book_summaries(store)?);
    summaries.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.remote_root.cmp(&b.remote_root))
    });
    summaries.dedup_by(|a, b| a.book_id == b.book_id && a.layout == b.layout);
    Ok(summaries)
}

fn human_readable_cloud_book_summaries(
    store: &OpenDalManagedObjectStore,
) -> Result<Vec<CloudBookSummary>, String> {
    let entries = store.list_recursive(HUMAN_READABLE_CLOUD_ROOT)?;
    let mut summaries = Vec::new();
    for entry in entries {
        if !entry.path.ends_with(HUMAN_READABLE_BOOK_META_SUFFIX) {
            continue;
        }
        let bytes = store.get(&entry.path).map_err(|e| e.to_string())?;
        let metadata = book_metadata_from_markdown_bytes(&bytes)?;
        let Some(remote_root) = entry.path.strip_suffix(HUMAN_READABLE_BOOK_META_SUFFIX) else {
            continue;
        };
        summaries.push(CloudBookSummary {
            book_id: metadata.book_id,
            name: metadata.name,
            updated_at: Utc::now().to_rfc3339(),
            remote_root: remote_root.to_string(),
            layout: CLOUD_BOOK_LAYOUT_HUMAN_READABLE.to_string(),
        });
    }
    Ok(summaries)
}

fn legacy_managed_cloud_book_summaries(
    store: &OpenDalManagedObjectStore,
) -> Result<Vec<CloudBookSummary>, String> {
    let entries = store.list_recursive("syllepsis-sync/books/")?;
    let mut summaries = Vec::new();
    for entry in entries {
        if !entry.path.ends_with("/manifest.json") && !entry.path.ends_with("manifest.json") {
            continue;
        }
        let bytes = store.get(&entry.path).map_err(|e| e.to_string())?;
        let manifest: syllepsis_core::sync::BookManifest =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let Some(remote_root) = entry.path.strip_suffix("manifest.json") else {
            continue;
        };
        summaries.push(CloudBookSummary {
            book_id: manifest.book_id,
            name: manifest.name,
            updated_at: manifest.updated_at.to_rfc3339(),
            remote_root: remote_root.to_string(),
            layout: CLOUD_BOOK_LAYOUT_LEGACY_MANAGED.to_string(),
        });
    }
    Ok(summaries)
}

fn book_metadata_from_markdown_bytes(bytes: &[u8]) -> Result<BookMetadata, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("parse _book.md utf-8: {error}"))?;
    let (frontmatter, _) = split_frontmatter(text)
        .ok_or_else(|| "cloud _book.md is missing frontmatter".to_string())?;
    serde_yaml::from_str(&frontmatter).map_err(|error| format!("parse cloud _book.md: {error}"))
}

#[tauri::command]
pub fn upload_book_to_cloud(
    app: AppHandle,
    state: State<AppState>,
    provider: String,
) -> Result<SyncReport, String> {
    let report = upload_book_to_cloud_inner(&state, &provider)?;
    mark_cloud_sync_provider_connected(&app, &provider, true)?;
    set_active_cloud_provider_for_current_book(&state, &provider)?;
    Ok(report)
}

/// Payload of the `cloud-sync-finished` event emitted when a backgrounded "Sync now" completes.
#[derive(Debug, Clone, Serialize)]
pub struct CloudSyncFinished {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<SyncReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub fn sync_managed_cloud_now(app: AppHandle, provider: String) {
    // Run off the IPC worker so a slow network never ties it up; the book lock is already released
    // during I/O (see `upload_book_to_cloud_inner`), so the UI stays responsive. Report back via an
    // event instead of a return value.
    tauri::async_runtime::spawn_blocking(move || {
        let result = {
            let state = app.state::<AppState>();
            upload_book_to_cloud_inner(&state, &provider).and_then(|report| {
                mark_cloud_sync_provider_connected(&app, &provider, true)?;
                set_active_cloud_provider_for_current_book(&state, &provider)?;
                Ok(report)
            })
        };
        let payload = match result {
            Ok(report) => CloudSyncFinished {
                provider: provider.clone(),
                report: Some(report),
                error: None,
            },
            Err(error) => CloudSyncFinished {
                provider: provider.clone(),
                report: None,
                error: Some(error),
            },
        };
        if let Err(error) = app.emit("cloud-sync-finished", payload) {
            tracing::debug!(error = %error, "failed to emit cloud-sync-finished");
        }
    });
}

pub fn start_managed_cloud_auto_sync(app: AppHandle) {
    thread::spawn(move || loop {
        // Sync immediately on the first iteration (this is also the startup sync — the engine is
        // bidirectional, so one pass pulls remote changes), then poll on a relaxed cadence.
        {
            let state = app.state::<AppState>();
            if let Err(error) = sync_connected_managed_cloud_providers(&app, &state) {
                tracing::debug!(error = %error, "managed cloud auto-sync skipped");
            }
        }
        thread::sleep(MANAGED_CLOUD_AUTO_SYNC_INTERVAL);
    });
}

pub(crate) fn sync_connected_managed_cloud_providers(
    app: &AppHandle,
    state: &AppState,
) -> Result<(), String> {
    let Some(provider) = active_cloud_provider_for_current_book(state)? else {
        return Ok(());
    };
    let connected_provider_ids = load_cloud_sync_connection_markers(app)?;
    if !connected_provider_ids.contains(&provider) {
        return Ok(());
    }
    match upload_book_to_cloud_inner(state, &provider) {
        Ok(report) => tracing::info!(
            provider = %provider,
            pushed = report.pushed.len(),
            pulled = report.pulled.len(),
            merged = report.merged.len(),
            conflicted = report.conflicted.len(),
            "cloud auto-sync complete"
        ),
        Err(error) if error == "no book is open" || error == "sync is disabled" => {}
        Err(error) => return Err(format!("{provider}: {error}")),
    }
    Ok(())
}

fn upload_book_to_cloud_inner(state: &AppState, provider: &str) -> Result<SyncReport, String> {
    // Coalesce overlapping syncs: hold a dedicated lock no UI command contends on. If another sync
    // is already running, skip this one (an empty report) rather than queue behind it.
    let _sync_guard = match state.sync_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => return Ok(SyncReport::default()),
    };

    // Briefly lock the book only to snapshot what the engine needs (it operates on files, not the
    // `Book`), then drop the guard so no other Tauri command stalls during network I/O.
    let (root, book_id, sync_cfg, actor, store, author) = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        if !book.config.sync.enabled {
            return Err("sync is disabled".to_string());
        }
        let store = opendal_sync_provider_for(state, provider, book)?;
        let actor = syllepsis_core::sync::actor_id_for(&book.root).map_err(|e| e.to_string())?;
        let author = if book.config.sync.author.trim().is_empty() {
            actor.as_str().to_string()
        } else {
            book.config.sync.author.clone()
        };
        (
            book.root.clone(),
            book.metadata.book_id.clone(),
            book.config.sync.clone(),
            actor,
            store,
            author,
        )
    };

    // Run the sync with NO book lock held — this is what keeps the UI responsive.
    let report = SyncEngine::new_human_readable_remote(
        root,
        Box::new(store),
        actor,
        &sync_cfg,
        book_id.clone(),
        author,
    )
    .sync()
    .map_err(|e| e.to_string())?;

    // Legacy managed-cloud cleanup (network I/O, but needs no book lock).
    if let Err(error) = delete_cloud_book_prefix(state, provider, &book_id) {
        tracing::debug!(
            provider = %provider,
            book_id = %book_id,
            error = %error,
            "legacy managed cloud cleanup skipped"
        );
    }

    // Re-lock briefly to refresh derived state — but only if the same book is still open (guard
    // against the user switching books mid-sync).
    let guard = state.book.lock().unwrap();
    if let Some(book) = guard.as_ref() {
        if book.metadata.book_id == book_id {
            book.store.refresh().map_err(|e| e.to_string())?;
            let _ = state.local_ai.enqueue_all_stale(book, false);
            state.invalidate_graph_corpus();
        }
    }
    Ok(report)
}

#[tauri::command]
pub fn delete_current_book(
    app: AppHandle,
    state: State<AppState>,
    expected_book_name: String,
) -> Result<DeleteCurrentBookReport, String> {
    let (book_name, book_path, book_id, book_root) = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        (
            book.metadata.name.clone(),
            book.root.display().to_string(),
            book.metadata.book_id.clone(),
            book.root.clone(),
        )
    };

    if expected_book_name != book_name {
        return Err("confirmation did not match the current notebook name".to_string());
    }

    let cloud_cleanup =
        delete_managed_cloud_data_for_connected_providers(&state, &book_root, &book_id);

    *state.file_watcher.lock().unwrap() = None;
    crate::commands::book::forget_tracked_book(app, book_path.clone())?;
    fs::remove_dir_all(&book_root)
        .map_err(|error| format!("delete notebook folder from disk: {error}"))?;
    *state.book.lock().unwrap() = None;
    state.invalidate_llm_service();

    Ok(DeleteCurrentBookReport {
        book_name,
        book_path,
        cloud_cleanup,
    })
}

#[tauri::command]
pub fn open_cloud_book(
    app: AppHandle,
    state: State<AppState>,
    provider: String,
    book_id: String,
    remote_root: String,
    layout: String,
    parent_path: String,
) -> Result<crate::commands::book::BookInfo, String> {
    let store = opendal_store_for(&state, &provider)?;
    match layout.as_str() {
        CLOUD_BOOK_LAYOUT_HUMAN_READABLE => open_human_readable_cloud_book(
            &app,
            &state,
            &provider,
            &book_id,
            &remote_root,
            &parent_path,
            store,
        ),
        CLOUD_BOOK_LAYOUT_LEGACY_MANAGED => open_legacy_managed_cloud_book(
            &app,
            &state,
            &provider,
            &book_id,
            &remote_root,
            &parent_path,
            store,
        ),
        other => Err(format!("unknown cloud book layout: {other}")),
    }
}

fn open_human_readable_cloud_book(
    app: &AppHandle,
    state: &AppState,
    provider: &str,
    book_id: &str,
    remote_root: &str,
    parent_path: &str,
    store: OpenDalManagedObjectStore,
) -> Result<crate::commands::book::BookInfo, String> {
    let meta_path = format!("{remote_root}{HUMAN_READABLE_BOOK_META_SUFFIX}");
    let metadata =
        book_metadata_from_markdown_bytes(&store.get(&meta_path).map_err(|e| e.to_string())?)?;
    if metadata.book_id != book_id {
        return Err("selected cloud notebook metadata did not match requested book id".to_string());
    }
    let root = local_cloud_book_root(Path::new(parent_path), &metadata)?;
    if root.exists() {
        let book = Book::open(&root).map_err(|e| e.to_string())?;
        if book.metadata.book_id != metadata.book_id {
            return Err(format!(
                "local folder {} already contains a different notebook",
                root.display()
            ));
        }
        let op = opendal_operator_for(
            state,
            provider,
            &operator_root_from_remote_root(remote_root),
        )?;
        let sync_provider = OpenDalSyncProvider {
            provider: provider.to_string(),
            op,
        };
        let actor = syllepsis_core::sync::actor_id_for(&book.root).map_err(|e| e.to_string())?;
        let author = if book.config.sync.author.trim().is_empty() {
            actor.as_str().to_string()
        } else {
            book.config.sync.author.clone()
        };
        SyncEngine::new_human_readable_remote(
            book.root.clone(),
            Box::new(sync_provider),
            actor,
            &book.config.sync,
            book.metadata.book_id.clone(),
            author,
        )
        .sync()
        .map_err(|e| e.to_string())?;
    } else {
        download_human_readable_cloud_book(&store, remote_root, &root)?;
    }
    open_downloaded_cloud_book(app, state, root, provider)
}

fn open_legacy_managed_cloud_book(
    app: &AppHandle,
    state: &AppState,
    provider: &str,
    book_id: &str,
    remote_root: &str,
    parent_path: &str,
    store: OpenDalManagedObjectStore,
) -> Result<crate::commands::book::BookInfo, String> {
    let manifest_path = format!("{remote_root}manifest.json");
    let manifest: syllepsis_core::sync::BookManifest =
        serde_json::from_slice(&store.get(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest.book_id != book_id {
        return Err("selected cloud notebook manifest did not match requested book id".to_string());
    }
    let metadata = BookMetadata {
        book_id: manifest.book_id.clone(),
        name: manifest.name.clone(),
        ..BookMetadata::new(&manifest.name)
    };
    let root = local_cloud_book_root(Path::new(parent_path), &metadata)?;
    let mut book = if root.exists() {
        let book = Book::open(&root).map_err(|e| e.to_string())?;
        if book.metadata.book_id != manifest.book_id {
            return Err(format!(
                "local folder {} already contains a different notebook",
                root.display()
            ));
        }
        book
    } else {
        Book::create(&root, &manifest.name).map_err(|e| e.to_string())?
    };
    book.metadata.book_id = manifest.book_id;
    book.save_metadata().map_err(|e| e.to_string())?;
    let mut engine = ManagedCloudSyncEngine::new(&book, store, provider);
    engine.sync().map_err(|e| e.to_string())?;
    open_downloaded_cloud_book(app, state, root, provider)
}

fn open_downloaded_cloud_book(
    app: &AppHandle,
    state: &AppState,
    root: PathBuf,
    provider: &str,
) -> Result<crate::commands::book::BookInfo, String> {
    let models_root = app
        .path()
        .app_data_dir()
        .map(|app_data_dir| crate::state::models_root_from_app_data(&app_data_dir))
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let book = Book::open(&root)
        .map(|book| book.with_models_root(models_root))
        .map_err(|e| e.to_string())?;
    let info = crate::commands::book::BookInfo {
        name: book.metadata.name.clone(),
        path: root.display().to_string(),
        open_warning: book.open_warning.as_ref().map(|warning| {
            crate::commands::book::BookOpenWarningInfo {
                missing_reserved_files: warning.missing_reserved_files.clone(),
                should_offer_create_here: warning.should_offer_create_here(),
            }
        }),
    };
    crate::commands::book::track_book_path(app, &root)?;
    save_active_cloud_provider_for_book_root(&root, Some(provider))?;
    *state.book.lock().unwrap() = Some(book);
    state.invalidate_llm_service();
    state.invalidate_graph_corpus();
    if let Some(book) = state.book.lock().unwrap().as_ref() {
        let _ = syllepsis_core::app::lifecycle::purge_expired_now(book);
        let _ = state.local_ai.enqueue_all_stale(book, false);
    }
    Ok(info)
}

fn download_human_readable_cloud_book(
    store: &OpenDalManagedObjectStore,
    remote_root: &str,
    local_root: &Path,
) -> Result<(), String> {
    let entries = store.list_recursive(remote_root)?;
    fs::create_dir_all(local_root).map_err(|error| {
        format!(
            "create local notebook folder {}: {error}",
            local_root.display()
        )
    })?;
    for entry in entries {
        let Some(rel) = entry.path.strip_prefix(remote_root) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let destination = local_root.join(rel);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create local cloud notebook directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let bytes = store.get(&entry.path).map_err(|e| e.to_string())?;
        fs::write(&destination, bytes).map_err(|error| {
            format!(
                "write local cloud notebook file {}: {error}",
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn local_cloud_book_root(parent_path: &Path, metadata: &BookMetadata) -> Result<PathBuf, String> {
    let preferred = parent_path.join(safe_book_folder_name(&metadata.name));
    if !preferred.exists() {
        return Ok(preferred);
    }
    if local_book_id(&preferred)?.as_deref() == Some(&metadata.book_id) {
        return Ok(preferred);
    }
    unique_cloud_book_root(parent_path, &metadata.name)
}

fn local_book_id(root: &Path) -> Result<Option<String>, String> {
    if !root.is_dir() {
        return Ok(None);
    }
    match Book::open(root) {
        Ok(book) => Ok(Some(book.metadata.book_id)),
        Err(_) => Ok(None),
    }
}

fn unique_cloud_book_root(parent_path: &Path, name: &str) -> Result<PathBuf, String> {
    let base = safe_book_folder_name(name);
    for index in 2..1000 {
        let candidate = parent_path.join(format!("{base}-{index}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not find an unused local folder for notebook {name:?} under {}",
        parent_path.display()
    ))
}

fn operator_root_from_remote_root(remote_root: &str) -> String {
    format!("/{}", remote_root.trim_start_matches('/'))
}

struct KeyringSyncCredentialStore;

trait SyncCredentialStore {
    fn get(&mut self, account: &str) -> Result<Option<String>, String>;
    fn set(&mut self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&mut self, account: &str) -> Result<(), String>;
}

impl SyncCredentialStore for KeyringSyncCredentialStore {
    fn get(&mut self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(sync_keychain_service(), account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("read keychain entry: {e}")),
        }
    }

    fn set(&mut self, account: &str, secret: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(sync_keychain_service(), account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        entry
            .set_password(secret)
            .map_err(|e| format!("write keychain entry: {e}"))
    }

    fn delete(&mut self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(sync_keychain_service(), account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("delete keychain entry: {e}")),
        }
    }
}

fn sync_keychain_service() -> &'static str {
    if cfg!(debug_assertions) {
        DEVELOPMENT_SYNC_KEYCHAIN_SERVICE
    } else {
        SYNC_KEYCHAIN_SERVICE
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CloudSyncConnectionMarkers {
    providers: BTreeSet<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct ActiveCloudProviderMarker {
    provider: Option<String>,
}

fn active_cloud_provider_path(book_root: &Path) -> PathBuf {
    layout::sync_dir(book_root).join(ACTIVE_CLOUD_PROVIDER_FILE)
}

fn active_cloud_provider_for_book_root(book_root: &Path) -> Result<Option<String>, String> {
    let path = active_cloud_provider_path(book_root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let inferred = infer_single_known_cloud_provider_for_book_root(book_root);
            if let Some(provider) = inferred.as_deref() {
                save_active_cloud_provider_for_book_root(book_root, Some(provider))?;
            }
            return Ok(inferred);
        }
        Err(error) => {
            return Err(format!(
                "read active cloud provider from {}: {error}",
                path.display()
            ))
        }
    };
    let marker: ActiveCloudProviderMarker = serde_json::from_str(&text).map_err(|error| {
        format!(
            "parse active cloud provider from {}: {error}",
            path.display()
        )
    })?;
    Ok(marker.provider)
}

fn infer_single_known_cloud_provider_for_book_root(book_root: &Path) -> Option<String> {
    let mut providers = fs::read_dir(layout::sync_dir(book_root))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_prefix("state-")
                .and_then(|value| value.strip_suffix(".json"))
                .or_else(|| {
                    name.strip_prefix(MANAGED_CLOUD_STATE_FILE_PREFIX)
                        .and_then(|value| value.strip_suffix(MANAGED_CLOUD_STATE_FILE_SUFFIX))
                })
                .map(str::to_string)
        })
        .filter(|provider| descriptor_for(provider).is_ok())
        .collect::<Vec<_>>();
    providers.sort();
    providers.dedup();
    match providers.as_slice() {
        [provider] => Some(provider.clone()),
        _ => None,
    }
}

fn save_active_cloud_provider_for_book_root(
    book_root: &Path,
    provider: Option<&str>,
) -> Result<(), String> {
    let path = active_cloud_provider_path(book_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create active cloud provider directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let marker = ActiveCloudProviderMarker {
        provider: provider.map(str::to_string),
    };
    let text = serde_json::to_string_pretty(&marker)
        .map_err(|error| format!("serialize active cloud provider: {error}"))?;
    fs::write(&path, text)
        .map_err(|error| format!("write active cloud provider to {}: {error}", path.display()))
}

fn active_cloud_provider_for_current_book(state: &AppState) -> Result<Option<String>, String> {
    let guard = state.book.lock().unwrap();
    let Some(book) = guard.as_ref() else {
        return Ok(None);
    };
    active_cloud_provider_for_book_root(&book.root)
}

fn set_active_cloud_provider_for_current_book(
    state: &AppState,
    provider: &str,
) -> Result<(), String> {
    descriptor_for(provider)?;
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    save_active_cloud_provider_for_book_root(&book.root, Some(provider))
}

fn clear_active_cloud_provider_if_matches_current_book(
    state: &AppState,
    provider: &str,
) -> Result<(), String> {
    let guard = state.book.lock().unwrap();
    let Some(book) = guard.as_ref() else {
        return Ok(());
    };
    if active_cloud_provider_for_book_root(&book.root)?.as_deref() == Some(provider) {
        save_active_cloud_provider_for_book_root(&book.root, None)?;
    }
    Ok(())
}

fn cloud_sync_connection_markers_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(CLOUD_SYNC_CONNECTION_MARKERS_FILE))
        .map_err(|error| format!("resolve cloud sync connection marker path: {error}"))
}

fn load_cloud_sync_connection_markers(app: &AppHandle) -> Result<BTreeSet<String>, String> {
    let path = cloud_sync_connection_markers_path(app)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(format!(
                "read cloud sync connection markers from {}: {error}",
                path.display()
            ))
        }
    };
    let markers: CloudSyncConnectionMarkers = serde_json::from_str(&text).map_err(|error| {
        format!(
            "parse cloud sync connection markers from {}: {error}",
            path.display()
        )
    })?;
    Ok(markers.providers)
}

fn save_cloud_sync_connection_markers(
    app: &AppHandle,
    providers: BTreeSet<String>,
) -> Result<(), String> {
    let path = cloud_sync_connection_markers_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create cloud sync connection marker directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&CloudSyncConnectionMarkers { providers })
        .map_err(|error| format!("serialize cloud sync connection markers: {error}"))?;
    fs::write(&path, text).map_err(|error| {
        format!(
            "write cloud sync connection markers to {}: {error}",
            path.display()
        )
    })
}

fn mark_cloud_sync_provider_connected(
    app: &AppHandle,
    provider: &str,
    connected: bool,
) -> Result<(), String> {
    descriptor_for(provider)?;
    let mut providers = load_cloud_sync_connection_markers(app)?;
    if connected {
        providers.insert(provider.to_string());
    } else {
        providers.remove(provider);
    }
    save_cloud_sync_connection_markers(app, providers)
}

struct OpenDalManagedObjectStore {
    op: opendal::blocking::Operator,
}

impl OpenDalManagedObjectStore {
    fn list_recursive(&self, prefix: &str) -> Result<Vec<ManagedObjectEntry>, String> {
        let entries = self
            .op
            .list_options(
                prefix,
                opendal::options::ListOptions {
                    recursive: true,
                    ..Default::default()
                },
            )
            .map_err(|e| format!("opendal recursive list {prefix}: {e}"))?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.metadata().mode() == opendal::EntryMode::FILE)
            .map(|entry| ManagedObjectEntry {
                path: entry.path().to_string(),
                size: entry.metadata().content_length(),
            })
            .collect())
    }
}

impl ManagedObjectStore for OpenDalManagedObjectStore {
    fn list(&self, prefix: &str) -> syllepsis_core::CoreResult<Vec<ManagedObjectEntry>> {
        let entries = self
            .op
            .list(prefix)
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal list {prefix}: {e}")))?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.metadata().mode() == opendal::EntryMode::FILE)
            .map(|entry| ManagedObjectEntry {
                path: entry.path().to_string(),
                size: entry.metadata().content_length(),
            })
            .collect())
    }

    fn get(&self, path: &str) -> syllepsis_core::CoreResult<Vec<u8>> {
        self.op
            .read(path)
            .map(|buffer| buffer.to_vec())
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal read {path}: {e}")))
    }

    fn put(&mut self, path: &str, bytes: &[u8]) -> syllepsis_core::CoreResult<()> {
        self.op
            .write(path, bytes.to_vec())
            .map(|_| ())
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal write {path}: {e}")))
    }

    fn delete(&mut self, path: &str) -> syllepsis_core::CoreResult<()> {
        self.op
            .delete(path)
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal delete {path}: {e}")))
    }
}

struct OpenDalSyncProvider {
    provider: String,
    op: opendal::blocking::Operator,
}

impl SyncProvider for OpenDalSyncProvider {
    fn name(&self) -> &str {
        &self.provider
    }

    fn list(&self) -> syllepsis_core::CoreResult<Vec<RemoteEntry>> {
        // List recursively (so files in subdirs like `_categories/` are seen) without downloading
        // any content. Each device's index fragments under `_sync_index/` carry the revisions, so
        // most files need no `get()` at all — only the ones the planner will actually pull.
        let entries = self
            .op
            .list_options(
                "",
                opendal::options::ListOptions {
                    recursive: true,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                syllepsis_core::CoreError::Sync(format!("opendal recursive list cloud root: {e}"))
            })?;
        let mut listed = Vec::new();
        let mut fragments = Vec::new();
        for entry in entries {
            if entry.metadata().mode() != opendal::EntryMode::FILE {
                continue;
            }
            let path = entry.path().trim_start_matches('/').to_string();
            if path.is_empty() {
                continue;
            }
            if is_cloud_index_path(&path) {
                if path.ends_with(".json") {
                    if let Ok(bytes) = self.get(&path) {
                        if let Ok(fragment) = serde_json::from_slice::<CloudIndexFragment>(&bytes) {
                            fragments.push(fragment);
                        }
                    }
                }
                continue;
            }
            listed.push(ListedRemoteFile {
                path,
                size: entry.metadata().content_length(),
            });
        }
        let index = CloudIndex::merge(fragments);
        build_remote_entries(listed, &index, |path| Ok(content_revision(&self.get(path)?)))
    }

    fn get(&self, path: &str) -> syllepsis_core::CoreResult<Vec<u8>> {
        self.op
            .read(path)
            .map(|buffer| buffer.to_vec())
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal read {path}: {e}")))
    }

    fn put(&self, path: &str, bytes: &[u8]) -> syllepsis_core::CoreResult<RemoteRevision> {
        self.op
            .write(path, bytes.to_vec())
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal write {path}: {e}")))?;
        Ok(content_revision(bytes))
    }

    fn delete(&self, path: &str) -> syllepsis_core::CoreResult<()> {
        self.op
            .delete(path)
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal delete {path}: {e}")))
    }

    fn publish_index(
        &self,
        actor: &str,
        author: &str,
        book_id: &str,
        entries: &std::collections::BTreeMap<String, IndexEntry>,
    ) -> syllepsis_core::CoreResult<()> {
        let fragment = CloudIndexFragment::new(book_id, actor, author, entries.clone());
        let path = fragment_path(author, actor);
        let bytes = serde_json::to_vec_pretty(&fragment).map_err(|e| {
            syllepsis_core::CoreError::Sync(format!("serialize cloud index fragment: {e}"))
        })?;
        self.op
            .write(&path, bytes)
            .map(|_| ())
            .map_err(|e| syllepsis_core::CoreError::Sync(format!("opendal write {path}: {e}")))
    }
}

fn opendal_sync_provider_for(
    state: &AppState,
    provider: &str,
    book: &Book,
) -> Result<OpenDalSyncProvider, String> {
    let root = cloud_book_sync_root(book);
    let op = opendal_operator_for(state, provider, &root)?;
    Ok(OpenDalSyncProvider {
        provider: provider.to_string(),
        op,
    })
}

fn opendal_store_for(
    state: &AppState,
    provider: &str,
) -> Result<OpenDalManagedObjectStore, String> {
    opendal_operator_for(state, provider, "/").map(|op| OpenDalManagedObjectStore { op })
}

fn opendal_operator_for(
    state: &AppState,
    provider: &str,
    root: &str,
) -> Result<opendal::blocking::Operator, String> {
    let credentials = credentials_for(state, provider)?;
    let op = match provider {
        "google_drive" => {
            let mut builder = opendal::services::Gdrive::default();
            builder = builder.root(root);
            apply_opendal_tokens_gdrive(builder, &credentials)?
        }
        "dropbox" => {
            let mut builder = opendal::services::Dropbox::default();
            builder = builder.root(root);
            apply_opendal_tokens_dropbox(builder, &credentials)?
        }
        "onedrive" => {
            let mut builder = opendal::services::Onedrive::default();
            builder = builder.root(root);
            apply_opendal_tokens_onedrive(builder, &credentials)?
        }
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    let runtime_handle = tauri::async_runtime::handle();
    let _runtime_guard = runtime_handle.inner().enter();
    opendal::blocking::Operator::new(op)
        .map_err(|e| format!("create blocking OpenDAL operator: {e}"))
}

fn cloud_book_sync_root(book: &Book) -> String {
    format!("/Syllepsis/{}/", safe_folder_name(&book.metadata.name))
}

fn delete_managed_cloud_data_for_connected_providers(
    state: &AppState,
    book_root: &Path,
    book_id: &str,
) -> Vec<DeleteBookCloudCleanupOutcome> {
    let providers = managed_cloud_state_providers_for_book(book_root);
    providers
        .into_iter()
        .map(|provider| {
            let connected = match credentials_for(state, &provider) {
                Ok(_) => true,
                Err(error) if error == format!("{provider} is not connected") => false,
                Err(error) => {
                    return DeleteBookCloudCleanupOutcome {
                        provider,
                        attempted: false,
                        connected: false,
                        deleted_object_count: 0,
                        error: Some(error),
                    }
                }
            };
            if !connected {
                return DeleteBookCloudCleanupOutcome {
                    provider,
                    attempted: false,
                    connected: false,
                    deleted_object_count: 0,
                    error: None,
                };
            }
            match delete_cloud_book_prefix(state, &provider, book_id) {
                Ok(deleted_object_count) => DeleteBookCloudCleanupOutcome {
                    provider,
                    attempted: true,
                    connected: true,
                    deleted_object_count,
                    error: None,
                },
                Err(error) => DeleteBookCloudCleanupOutcome {
                    provider,
                    attempted: true,
                    connected: true,
                    deleted_object_count: 0,
                    error: Some(error),
                },
            }
        })
        .collect()
}

fn managed_cloud_state_providers_for_book(book_root: &Path) -> Vec<String> {
    let mut providers = fs::read_dir(layout::sync_dir(book_root))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            if !name.starts_with(MANAGED_CLOUD_STATE_FILE_PREFIX)
                || !name.ends_with(MANAGED_CLOUD_STATE_FILE_SUFFIX)
            {
                return None;
            }
            let provider = name
                .trim_start_matches(MANAGED_CLOUD_STATE_FILE_PREFIX)
                .trim_end_matches(MANAGED_CLOUD_STATE_FILE_SUFFIX)
                .trim();
            if provider.is_empty() {
                None
            } else {
                Some(provider.to_string())
            }
        })
        .collect::<Vec<_>>();
    providers.sort();
    providers.dedup();
    providers
}

fn delete_cloud_book_prefix(
    state: &AppState,
    provider: &str,
    book_id: &str,
) -> Result<usize, String> {
    let mut store = opendal_store_for(state, provider)?;
    let prefix = format!("syllepsis-sync/books/{book_id}/");
    let entries = store.list(&prefix).map_err(|error| error.to_string())?;
    let mut deleted = 0_usize;
    for entry in entries {
        store
            .delete(&entry.path)
            .map_err(|error| format!("delete managed cloud object {}: {error}", entry.path))?;
        deleted += 1;
    }
    Ok(deleted)
}

struct CloudSyncOAuthCompletion {
    status: CloudSyncProviderStatus,
    credentials: CloudCredentials,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CloudCredentials {
    client_id: String,
    client_secret: Option<String>,
    access_token: Option<String>,
    access_token_expires_at: Option<SystemTime>,
    refresh_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CloudAccessToken {
    token: String,
    expires_at: Option<SystemTime>,
}

fn credentials_for(state: &AppState, provider: &str) -> Result<CloudCredentials, String> {
    let mut store = KeyringSyncCredentialStore;
    let mut refresh_access_token = refresh_google_drive_access_token;
    credentials_for_with_store_and_refresh(state, &mut store, provider, &mut refresh_access_token)
}

fn credentials_for_with_store_and_refresh(
    state: &AppState,
    store: &mut impl SyncCredentialStore,
    provider: &str,
    refresh_access_token: &mut impl FnMut(&str, Option<&str>, &str) -> Result<CloudAccessToken, String>,
) -> Result<CloudCredentials, String> {
    descriptor_for(provider)?;
    if let Some(mut credentials) = cached_sync_credentials(state, provider) {
        refresh_google_drive_access_token_if_needed(
            provider,
            &mut credentials,
            refresh_access_token,
        )?;
        cache_sync_credentials(state, provider, &credentials);
        return Ok(credentials);
    }
    let oauth_client_config = oauth_client_config(provider)?;
    let mut credentials = CloudCredentials {
        client_id: oauth_client_config.client_id.trim().to_string(),
        client_secret: oauth_client_config
            .client_secret
            .map(|secret| secret.trim().to_string())
            .filter(|secret| !secret.is_empty()),
        access_token: token_for_field(store, provider, ACCESS_TOKEN_FIELD)?,
        access_token_expires_at: None,
        refresh_token: token_for_field(store, provider, REFRESH_TOKEN_FIELD)?,
    };
    if credentials.access_token.is_none() && credentials.refresh_token.is_none() {
        return Err(format!("{provider} is not connected"));
    }
    refresh_google_drive_access_token_if_needed(provider, &mut credentials, refresh_access_token)?;
    cache_sync_credentials(state, provider, &credentials);
    Ok(credentials)
}

fn refresh_google_drive_access_token_if_needed(
    provider: &str,
    credentials: &mut CloudCredentials,
    refresh_access_token: &mut impl FnMut(&str, Option<&str>, &str) -> Result<CloudAccessToken, String>,
) -> Result<(), String> {
    if provider != "google_drive" || google_drive_access_token_is_current(credentials) {
        return Ok(());
    }
    let Some(refresh_token) = credentials.refresh_token.as_deref() else {
        return Ok(());
    };
    let access_token = refresh_access_token(
        &credentials.client_id,
        credentials.client_secret.as_deref(),
        refresh_token,
    )?;
    credentials.access_token = Some(access_token.token);
    credentials.access_token_expires_at = access_token.expires_at;
    Ok(())
}

fn google_drive_access_token_is_current(credentials: &CloudCredentials) -> bool {
    if credentials.access_token.is_none() {
        return false;
    }
    let Some(expires_at) = credentials.access_token_expires_at else {
        return credentials.refresh_token.is_none();
    };
    expires_at
        .duration_since(SystemTime::now())
        .map(|remaining| remaining > ACCESS_TOKEN_REFRESH_SAFETY_WINDOW)
        .unwrap_or(false)
}

fn cloud_credentials_for_tokens(
    provider: &str,
    access_token: Option<String>,
    access_token_expires_at: Option<SystemTime>,
    refresh_token: Option<String>,
) -> Result<CloudCredentials, String> {
    let oauth_client_config = oauth_client_config(provider)?;
    Ok(CloudCredentials {
        client_id: oauth_client_config.client_id.trim().to_string(),
        client_secret: oauth_client_config
            .client_secret
            .map(|secret| secret.trim().to_string())
            .filter(|secret| !secret.is_empty()),
        access_token,
        access_token_expires_at,
        refresh_token,
    })
}

fn cached_sync_credentials(state: &AppState, provider: &str) -> Option<CloudCredentials> {
    let cache = state.cloud_sync_credentials.lock().unwrap();
    let credentials = cache.get(provider)?;
    Some(CloudCredentials {
        client_id: credentials.client_id.clone(),
        client_secret: credentials.client_secret.clone(),
        access_token: credentials.access_token.clone(),
        access_token_expires_at: credentials.access_token_expires_at,
        refresh_token: credentials.refresh_token.clone(),
    })
}

fn cache_sync_credentials(state: &AppState, provider: &str, credentials: &CloudCredentials) {
    state.cloud_sync_credentials.lock().unwrap().insert(
        provider.to_string(),
        CachedCloudSyncCredentials {
            client_id: credentials.client_id.clone(),
            client_secret: credentials.client_secret.clone(),
            access_token: credentials.access_token.clone(),
            access_token_expires_at: credentials.access_token_expires_at,
            refresh_token: credentials.refresh_token.clone(),
        },
    );
}

fn remove_cached_sync_credentials(state: &AppState, provider: &str) {
    state
        .cloud_sync_credentials
        .lock()
        .unwrap()
        .remove(provider);
}

fn token_for_field(
    store: &mut impl SyncCredentialStore,
    provider: &str,
    field: &str,
) -> Result<Option<String>, String> {
    Ok(store
        .get(&account(provider, field))?
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty()))
}

fn apply_opendal_tokens_gdrive(
    mut builder: opendal::services::Gdrive,
    credentials: &CloudCredentials,
) -> Result<opendal::Operator, String> {
    builder = builder.client_id(&credentials.client_id);
    if let Some(token) = &credentials.access_token {
        builder = builder.access_token(token);
    } else if let Some(token) = &credentials.refresh_token {
        builder = builder.refresh_token(token);
        builder = builder.client_secret(credentials.client_secret.as_deref().unwrap_or(""));
    }
    opendal::Operator::new(builder)
        .map(|builder| builder.finish())
        .map_err(|e| format!("build Google Drive operator: {e}"))
}

fn apply_opendal_tokens_dropbox(
    mut builder: opendal::services::Dropbox,
    credentials: &CloudCredentials,
) -> Result<opendal::Operator, String> {
    builder = builder.client_id(&credentials.client_id);
    if let Some(token) = &credentials.access_token {
        builder = builder.access_token(token);
    }
    if let Some(token) = &credentials.refresh_token {
        builder = builder.refresh_token(token);
    }
    opendal::Operator::new(builder)
        .map(|builder| builder.finish())
        .map_err(|e| format!("build Dropbox operator: {e}"))
}

fn apply_opendal_tokens_onedrive(
    mut builder: opendal::services::Onedrive,
    credentials: &CloudCredentials,
) -> Result<opendal::Operator, String> {
    builder = builder.client_id(&credentials.client_id);
    if let Some(token) = &credentials.access_token {
        builder = builder.access_token(token);
    }
    if let Some(token) = &credentials.refresh_token {
        builder = builder.refresh_token(token);
    }
    opendal::Operator::new(builder)
        .map(|builder| builder.finish())
        .map_err(|e| format!("build OneDrive operator: {e}"))
}

fn cloud_descriptors() -> Vec<CloudSyncProviderDescriptor> {
    vec![
        CloudSyncProviderDescriptor {
            provider: "google_drive".to_string(),
            display_name: "Google Drive".to_string(),
            auth_url_base: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        },
        CloudSyncProviderDescriptor {
            provider: "dropbox".to_string(),
            display_name: "Dropbox".to_string(),
            auth_url_base: "https://www.dropbox.com/oauth2/authorize".to_string(),
        },
        CloudSyncProviderDescriptor {
            provider: "onedrive".to_string(),
            display_name: "OneDrive".to_string(),
            auth_url_base: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                .to_string(),
        },
    ]
}

fn descriptor_for(provider: &str) -> Result<CloudSyncProviderDescriptor, String> {
    cloud_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.provider == provider)
        .ok_or_else(|| format!("unknown cloud sync provider: {provider}"))
}

#[derive(Deserialize)]
struct CloudSyncOAuthClientConfig {
    client_id: String,
    callback_port: u16,
    #[serde(default)]
    client_secret: Option<String>,
}

#[derive(Deserialize)]
struct CloudSyncOAuthClientIds {
    google_drive: CloudSyncOAuthClientConfig,
    dropbox: CloudSyncOAuthClientConfig,
    onedrive: CloudSyncOAuthClientConfig,
}

#[derive(Default, Deserialize)]
struct CloudSyncOAuthClientIdOverrides {
    #[serde(default)]
    google_drive: Option<CloudSyncOAuthClientConfigOverride>,
    #[serde(default)]
    dropbox: Option<CloudSyncOAuthClientConfigOverride>,
    #[serde(default)]
    onedrive: Option<CloudSyncOAuthClientConfigOverride>,
}

#[derive(Default, Deserialize)]
struct CloudSyncOAuthClientConfigOverride {
    client_id: Option<String>,
    callback_port: Option<u16>,
    client_secret: Option<String>,
}

fn oauth_client_config(provider: &str) -> Result<CloudSyncOAuthClientConfig, String> {
    let configured_client_ids: CloudSyncOAuthClientIds =
        serde_json::from_str(include_str!("../../oauth-client-ids.json"))
            .map_err(|error| format!("parse bundled OAuth client IDs: {error}"))?;
    let mut config = match provider {
        "google_drive" => configured_client_ids.google_drive,
        "dropbox" => configured_client_ids.dropbox,
        "onedrive" => configured_client_ids.onedrive,
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    apply_oauth_client_config_override(provider, &mut config)?;
    apply_oauth_client_secret_env(provider, &mut config);
    if config.client_id.trim().is_empty() {
        return Err(format!(
            "{provider} OAuth is not configured in this build; add the Syllepsis app client ID to crates/syllepsis-tauri/oauth-client-ids.json"
        ));
    }
    Ok(config)
}

fn apply_oauth_client_config_override(
    provider: &str,
    config: &mut CloudSyncOAuthClientConfig,
) -> Result<(), String> {
    let local_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(OAUTH_CLIENT_IDS_LOCAL_FILE);
    let text = match std::fs::read_to_string(&local_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "read local OAuth client IDs from {}: {error}",
                local_path.display()
            ))
        }
    };
    let overrides: CloudSyncOAuthClientIdOverrides =
        serde_json::from_str(&text).map_err(|error| {
            format!(
                "parse local OAuth client IDs from {}: {error}",
                local_path.display()
            )
        })?;
    let provider_override = match provider {
        "google_drive" => overrides.google_drive,
        "dropbox" => overrides.dropbox,
        "onedrive" => overrides.onedrive,
        _ => None,
    };
    if let Some(provider_override) = provider_override {
        apply_oauth_client_config_fields(config, provider_override);
    }
    Ok(())
}

fn apply_oauth_client_config_fields(
    config: &mut CloudSyncOAuthClientConfig,
    config_override: CloudSyncOAuthClientConfigOverride,
) {
    if let Some(client_id) = config_override.client_id {
        config.client_id = client_id;
    }
    if let Some(callback_port) = config_override.callback_port {
        config.callback_port = callback_port;
    }
    if let Some(client_secret) = config_override.client_secret {
        config.client_secret = Some(client_secret);
    }
}

fn apply_oauth_client_secret_env(provider: &str, config: &mut CloudSyncOAuthClientConfig) {
    let env_var = match provider {
        "google_drive" => GOOGLE_DRIVE_CLIENT_SECRET_ENV_VAR,
        _ => return,
    };
    if let Some(secret) = std::env::var_os(env_var)
        .and_then(|value| value.into_string().ok())
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
    {
        config.client_secret = Some(secret);
    }
}

fn require_oauth_client_id(provider: &str) -> Result<String, String> {
    Ok(oauth_client_config(provider)?.client_id.trim().to_string())
}

fn oauth_url(
    provider: &str,
    state: &str,
    pkce_challenge: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let descriptor = descriptor_for(provider)?;
    let client_id = require_oauth_client_id(provider)?;
    let scope = match provider {
        "google_drive" => "https://www.googleapis.com/auth/drive.file",
        "dropbox" => "files.content.write files.content.read files.metadata.read",
        "onedrive" => "Files.ReadWrite offline_access",
        _ => "",
    };
    // Provider-specific extras appended after the shared parameters.
    let extras = match provider {
        "google_drive" => "&access_type=offline&prompt=consent",
        "dropbox" => "&token_access_type=offline",
        _ => "",
    };
    Ok(format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256{}",
        descriptor.auth_url_base,
        percent_encode(&client_id),
        percent_encode(redirect_uri),
        percent_encode(scope),
        percent_encode(state),
        percent_encode(pkce_challenge),
        extras,
    ))
}

/// Generate a PKCE code verifier: two concatenated ULIDs give 52 unreserved-alphabet chars,
/// within the 43-128 range required by RFC 7636.
fn pkce_verifier() -> String {
    format!("{}{}", ulid::Ulid::new(), ulid::Ulid::new())
}

/// Compute the PKCE S256 code challenge: BASE64URL(SHA-256(verifier)), no padding.
fn pkce_challenge(verifier: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn token_endpoint(provider: &str) -> Result<&'static str, String> {
    match provider {
        "google_drive" => Ok("https://oauth2.googleapis.com/token"),
        "dropbox" => Ok("https://api.dropboxapi.com/oauth2/token"),
        "onedrive" => Ok("https://login.microsoftonline.com/common/oauth2/v2.0/token"),
        other => Err(format!("unknown cloud sync provider: {other}")),
    }
}

/// Exchange an OAuth authorization code for access/refresh tokens using PKCE.
fn exchange_code_for_tokens(
    provider: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<CloudCredentials, String> {
    let oauth_client_config = oauth_client_config(provider)?;
    let client_id = oauth_client_config.client_id.trim().to_string();
    let client_secret = oauth_client_config
        .client_secret
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty());
    let endpoint = token_endpoint(provider)?;

    // Build an application/x-www-form-urlencoded body without the `form` reqwest feature.
    let mut body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        percent_encode(code),
        percent_encode(redirect_uri),
        percent_encode(&client_id),
        percent_encode(verifier),
    );
    if let Some(secret) = client_secret.as_deref() {
        body.push_str("&client_secret=");
        body.push_str(&percent_encode(secret));
    }
    let response = reqwest::blocking::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| format!("token exchange request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(token_exchange_error(provider, status, &body));
    }
    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("token exchange response parse failed: {e}"))?;

    let access_token = json
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let access_token_expires_at = access_token_expires_at_from_json(&json);
    let refresh_token = json
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    if access_token.is_none() && refresh_token.is_none() {
        return Err(format!(
            "token exchange succeeded but returned no token: {json}"
        ));
    }
    Ok(CloudCredentials {
        client_id,
        client_secret,
        access_token,
        access_token_expires_at,
        refresh_token,
    })
}

fn token_exchange_error(provider: &str, status: reqwest::StatusCode, body: &str) -> String {
    let mut message = format!("token exchange returned {status}: {body}");
    if provider == "google_drive" && body.contains("client_secret is missing") {
        message.push_str(
            "\n\nGoogle rejected the token exchange because this OAuth client requires a client_secret. Set SYLLEPSIS_GOOGLE_DRIVE_CLIENT_SECRET or add google_drive.client_secret to the ignored crates/syllepsis-tauri/oauth-client-ids.local.json file, then choose Reconnect.",
        );
    }
    message
}

fn refresh_google_drive_access_token(
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<CloudAccessToken, String> {
    let mut body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        percent_encode(refresh_token),
        percent_encode(client_id),
    );
    if let Some(secret) = client_secret.filter(|secret| !secret.is_empty()) {
        body.push_str("&client_secret=");
        body.push_str(&percent_encode(secret));
    }
    let response = reqwest::blocking::Client::new()
        .post(token_endpoint("google_drive")?)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| format!("refresh Google Drive access token request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "refresh Google Drive access token returned {status}: {body}"
        ));
    }

    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("refresh Google Drive token response parse failed: {e}"))?;
    let token = json
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            format!("refresh Google Drive token response returned no access token: {json}")
        })?;
    Ok(CloudAccessToken {
        token,
        expires_at: access_token_expires_at_from_json(&json),
    })
}

fn access_token_expires_at_from_json(json: &serde_json::Value) -> Option<SystemTime> {
    json.get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .and_then(|seconds| SystemTime::now().checked_add(Duration::from_secs(seconds)))
}

fn receive_oauth_callback(listener: TcpListener) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("configure OAuth callback listener: {error}"))?;
    let deadline = Instant::now() + OAUTH_CALLBACK_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => return read_oauth_callback_request(&mut stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(
                        "OAuth authorization timed out; choose Reconnect to try again".to_string(),
                    );
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(format!("accept OAuth callback: {error}")),
        }
    }
}

fn read_oauth_callback_request(stream: &mut TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("configure OAuth callback connection: {error}"))?;
    let mut request_bytes = [0_u8; 8192];
    let bytes_read = stream
        .read(&mut request_bytes)
        .map_err(|error| format!("read OAuth callback: {error}"))?;
    let request = String::from_utf8_lossy(&request_bytes[..bytes_read]);
    let request_target = request
        .lines()
        .next()
        .and_then(|request_line| request_line.split_whitespace().nth(1))
        .ok_or_else(|| "OAuth callback did not contain a valid HTTP request".to_string())?;
    if !request_target.starts_with(OAUTH_CALLBACK_PATH) {
        return Err("OAuth callback used an unexpected path".to_string());
    }

    let response_body =
        "Authorization received. You can close this browser tab and return to Syllepsis.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write OAuth callback response: {error}"))?;

    let local_address = stream
        .local_addr()
        .map_err(|error| format!("read OAuth callback listener address: {error}"))?;
    Ok(format!("http://{local_address}{request_target}"))
}

fn parse_query_params(url: &str) -> std::collections::BTreeMap<String, String> {
    let query = url
        .split_once('?')
        .map(|(_, q)| q)
        .or_else(|| url.split_once('#').map(|(_, q)| q))
        .unwrap_or("");
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

fn account(provider: &str, field: &str) -> String {
    format!("{provider}:{field}")
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn watch_activity_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let rel = rel.to_str()?.replace('\\', "/");
    if should_ignore_watch_rel_path(&rel) {
        None
    } else {
        Some(rel)
    }
}

fn should_ignore_watch_rel_path(rel: &str) -> bool {
    let mut components = rel.split('/');
    let first = components.next().unwrap_or("");
    if first == "_sync" || first == "_derived" || first == "_crdt" {
        return true;
    }
    rel.split('/').any(is_ignored_watch_file_name)
}

fn is_ignored_watch_file_name(name: &str) -> bool {
    name.is_empty()
        || name.starts_with('.')
        || name.ends_with('~')
        || name.ends_with(".tmp")
        || name.ends_with(".temp")
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.contains(".sb-")
}

fn should_debounce_watch_activity(
    recent: &Arc<Mutex<HashMap<String, Instant>>>,
    rel: &str,
    now: Instant,
) -> bool {
    let mut recent = recent.lock().unwrap();
    let should_skip = recent
        .get(rel)
        .map(|previous| now.duration_since(*previous) < WATCH_ACTIVITY_DEBOUNCE)
        .unwrap_or(false);
    recent.insert(rel.to_string(), now);
    if recent.len() > 256 {
        recent.retain(|_, previous| now.duration_since(*previous) < WATCH_ACTIVITY_DEBOUNCE);
    }
    should_skip
}

fn watch_activity_kind(rel: &str) -> &'static str {
    if rel.contains(".conflict-") || rel.contains("Conflicted copy") {
        "conflict_detected"
    } else {
        "external_update"
    }
}

fn safe_folder_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn safe_book_folder_name(name: &str) -> String {
    let folder = safe_folder_name(name);
    if folder.is_empty() {
        "untitled-book".to_string()
    } else {
        folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::fs::{create_dir_all, write};
    use tempfile::tempdir;

    #[derive(Default)]
    struct FakeSyncCredentialStore {
        values: HashMap<String, String>,
        get_count: usize,
        set_count: usize,
        delete_count: usize,
    }

    impl SyncCredentialStore for FakeSyncCredentialStore {
        fn get(&mut self, account: &str) -> Result<Option<String>, String> {
            self.get_count += 1;
            Ok(self.values.get(account).cloned())
        }

        fn set(&mut self, account: &str, secret: &str) -> Result<(), String> {
            self.set_count += 1;
            self.values.insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&mut self, account: &str) -> Result<(), String> {
            self.delete_count += 1;
            self.values.remove(account);
            Ok(())
        }
    }

    #[test]
    fn sync_credentials_cache_prevents_repeated_store_reads() {
        let state = AppState::new();
        let mut store = FakeSyncCredentialStore::default();
        store.values.insert(
            account("dropbox", ACCESS_TOKEN_FIELD),
            "access-token".to_string(),
        );
        store.values.insert(
            account("dropbox", REFRESH_TOKEN_FIELD),
            "refresh-token".to_string(),
        );
        let mut refresh_access_token = |_: &str, _: Option<&str>, _: &str| {
            panic!("dropbox should not refresh Google access tokens")
        };

        let first = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "dropbox",
            &mut refresh_access_token,
        )
        .unwrap();
        assert_eq!(first.access_token.as_deref(), Some("access-token"));
        assert_eq!(first.refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(store.get_count, 2);

        let second = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "dropbox",
            &mut refresh_access_token,
        )
        .unwrap();
        assert_eq!(second.access_token.as_deref(), Some("access-token"));
        assert_eq!(store.get_count, 2);
    }

    #[test]
    fn cached_sync_credentials_can_be_cleared_for_disconnect() {
        let state = AppState::new();
        let credentials = cloud_credentials_for_tokens(
            "dropbox",
            Some("access-token".to_string()),
            None,
            Some("refresh-token".to_string()),
        )
        .unwrap();

        cache_sync_credentials(&state, "dropbox", &credentials);
        assert!(cached_sync_credentials(&state, "dropbox").is_some());

        remove_cached_sync_credentials(&state, "dropbox");
        assert!(cached_sync_credentials(&state, "dropbox").is_none());
    }

    #[test]
    fn google_access_token_refresh_respects_expiry_window() {
        let state = AppState::new();
        let valid_credentials = cloud_credentials_for_tokens(
            "google_drive",
            Some("cached-access".to_string()),
            Some(SystemTime::now() + Duration::from_secs(60 * 60)),
            Some("refresh-token".to_string()),
        )
        .unwrap();
        cache_sync_credentials(&state, "google_drive", &valid_credentials);
        let mut store = FakeSyncCredentialStore::default();
        let refresh_count = Cell::new(0_usize);
        let mut refresh_access_token = |_: &str, _: Option<&str>, _: &str| {
            refresh_count.set(refresh_count.get() + 1);
            Ok(CloudAccessToken {
                token: "new-access".to_string(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(60 * 60)),
            })
        };

        let reused = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "google_drive",
            &mut refresh_access_token,
        )
        .unwrap();
        assert_eq!(reused.access_token.as_deref(), Some("cached-access"));
        assert_eq!(refresh_count.get(), 0);

        let near_expiry_credentials = cloud_credentials_for_tokens(
            "google_drive",
            Some("almost-expired".to_string()),
            Some(SystemTime::now() + Duration::from_secs(60)),
            Some("refresh-token".to_string()),
        )
        .unwrap();
        cache_sync_credentials(&state, "google_drive", &near_expiry_credentials);

        let refreshed = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "google_drive",
            &mut refresh_access_token,
        )
        .unwrap();
        assert_eq!(refreshed.access_token.as_deref(), Some("new-access"));
        assert_eq!(refresh_count.get(), 1);

        let reused_refreshed = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "google_drive",
            &mut refresh_access_token,
        )
        .unwrap();
        assert_eq!(reused_refreshed.access_token.as_deref(), Some("new-access"));
        assert_eq!(refresh_count.get(), 1);
    }

    #[test]
    fn google_access_token_refreshes_when_missing() {
        let state = AppState::new();
        let missing_access_credentials = cloud_credentials_for_tokens(
            "google_drive",
            None,
            None,
            Some("refresh-token".to_string()),
        )
        .unwrap();
        cache_sync_credentials(&state, "google_drive", &missing_access_credentials);
        let mut store = FakeSyncCredentialStore::default();
        let refresh_count = Cell::new(0_usize);
        let mut refresh_access_token = |_: &str, _: Option<&str>, _: &str| {
            refresh_count.set(refresh_count.get() + 1);
            Ok(CloudAccessToken {
                token: "new-access".to_string(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(60 * 60)),
            })
        };

        let refreshed = credentials_for_with_store_and_refresh(
            &state,
            &mut store,
            "google_drive",
            &mut refresh_access_token,
        )
        .unwrap();

        assert_eq!(refreshed.access_token.as_deref(), Some("new-access"));
        assert_eq!(refresh_count.get(), 1);
    }

    #[test]
    fn watch_filter_ignores_local_sidecar_and_scratch_paths() {
        assert!(should_ignore_watch_rel_path("_sync/activity.jsonl"));
        assert!(should_ignore_watch_rel_path("_derived/search.db"));
        assert!(should_ignore_watch_rel_path(
            "_crdt/01KVR92EBCQZQZ4ENNG2HJQ5KB.crdt"
        ));
        assert!(should_ignore_watch_rel_path(
            "note-tasks-syllepsis-01KVR92EBCQZQZ4ENNG2HJQ5KB.md.sb-1b9b6d00"
        ));
        assert!(should_ignore_watch_rel_path(".note.swp"));
        assert!(should_ignore_watch_rel_path("note.md~"));
    }

    #[test]
    fn watch_filter_accepts_visible_note_paths() {
        assert!(!should_ignore_watch_rel_path(
            "note-tasks-syllepsis-01KVR92EBCQZQZ4ENNG2HJQ5KB.md"
        ));
        assert!(!should_ignore_watch_rel_path("_categories/tasks.md"));
    }

    #[test]
    fn watch_kind_detects_real_conflict_copies() {
        assert_eq!(
            watch_activity_kind("note-a-01KVR92EBCQZQZ4ENNG2HJQ5KB.conflict-ab12.md"),
            "conflict_detected"
        );
        assert_eq!(
            watch_activity_kind("note-a-01KVR92EBCQZQZ4ENNG2HJQ5KB.md"),
            "external_update"
        );
    }

    #[test]
    fn watch_debounce_collapses_repeated_path_events() {
        let recent = Arc::new(Mutex::new(HashMap::new()));
        let now = Instant::now();
        assert!(!should_debounce_watch_activity(&recent, "a.md", now));
        assert!(should_debounce_watch_activity(
            &recent,
            "a.md",
            now + Duration::from_millis(100)
        ));
        assert!(!should_debounce_watch_activity(
            &recent,
            "a.md",
            now + Duration::from_secs(2)
        ));
    }

    #[test]
    fn oauth_callback_listener_reads_loopback_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(
                    b"GET /oauth-callback?code=code-123&state=state-123 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                )
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });

        let (mut stream, _) = listener.accept().unwrap();
        let callback_url = read_oauth_callback_request(&mut stream).unwrap();
        assert!(callback_url.ends_with("/oauth-callback?code=code-123&state=state-123"));
        drop(stream);
        assert!(client.join().unwrap().contains("Authorization received"));
    }

    #[test]
    fn bundled_oauth_callback_ports_are_distinct() {
        let configured: CloudSyncOAuthClientIds =
            serde_json::from_str(include_str!("../../oauth-client-ids.json")).unwrap();
        let ports = [
            configured.google_drive.callback_port,
            configured.dropbox.callback_port,
            configured.onedrive.callback_port,
        ];
        assert!(ports.iter().all(|port| *port >= 1024));
        assert_ne!(ports[0], ports[1]);
        assert_ne!(ports[0], ports[2]);
        assert_ne!(ports[1], ports[2]);
    }

    #[test]
    fn google_token_exchange_secret_error_names_config_field() {
        let message = token_exchange_error(
            "google_drive",
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_request","error_description":"client_secret is missing."}"#,
        );

        assert!(message.contains("google_drive.client_secret"));
        assert!(message.contains(GOOGLE_DRIVE_CLIENT_SECRET_ENV_VAR));
        assert!(message.contains(OAUTH_CLIENT_IDS_LOCAL_FILE));
    }

    #[test]
    fn managed_cloud_provider_discovery_reads_state_files() {
        let temp = tempdir().unwrap();
        let sync_dir = layout::sync_dir(temp.path());
        create_dir_all(&sync_dir).unwrap();
        write(sync_dir.join("managed-cloud-google_drive.json"), b"{}").unwrap();
        write(sync_dir.join("managed-cloud-dropbox.json"), b"{}").unwrap();
        write(sync_dir.join("managed-cloud-dropbox.json.bak"), b"{}").unwrap();
        write(sync_dir.join("activity.jsonl"), b"{}").unwrap();

        let providers = managed_cloud_state_providers_for_book(temp.path());

        assert_eq!(providers, vec!["dropbox", "google_drive"]);
    }

    #[test]
    fn active_cloud_provider_marker_round_trips() {
        let temp = tempdir().unwrap();

        assert_eq!(
            active_cloud_provider_for_book_root(temp.path()).unwrap(),
            None
        );
        save_active_cloud_provider_for_book_root(temp.path(), Some("google_drive")).unwrap();
        assert_eq!(
            active_cloud_provider_for_book_root(temp.path())
                .unwrap()
                .as_deref(),
            Some("google_drive")
        );
        save_active_cloud_provider_for_book_root(temp.path(), None).unwrap();
        assert_eq!(
            active_cloud_provider_for_book_root(temp.path()).unwrap(),
            None
        );
    }

    #[test]
    fn missing_active_marker_infers_single_known_cloud_provider() {
        let temp = tempdir().unwrap();
        let sync_dir = layout::sync_dir(temp.path());
        create_dir_all(&sync_dir).unwrap();
        write(sync_dir.join("state-google_drive.json"), b"{}").unwrap();

        assert_eq!(
            active_cloud_provider_for_book_root(temp.path())
                .unwrap()
                .as_deref(),
            Some("google_drive")
        );
        assert_eq!(
            active_cloud_provider_for_book_root(temp.path())
                .unwrap()
                .as_deref(),
            Some("google_drive")
        );
    }

    #[test]
    fn missing_active_marker_does_not_guess_between_multiple_cloud_providers() {
        let temp = tempdir().unwrap();
        let sync_dir = layout::sync_dir(temp.path());
        create_dir_all(&sync_dir).unwrap();
        write(sync_dir.join("state-google_drive.json"), b"{}").unwrap();
        write(sync_dir.join("state-dropbox.json"), b"{}").unwrap();

        assert_eq!(
            active_cloud_provider_for_book_root(temp.path()).unwrap(),
            None
        );
    }

    #[test]
    fn cloud_book_metadata_parses_human_readable_book_file() {
        let metadata = BookMetadata {
            book_id: "book-123".to_string(),
            name: "Field Notes".to_string(),
            ..BookMetadata::new("ignored")
        };
        let yaml = serde_yaml::to_string(&metadata).unwrap();
        let bytes = format!("---\n{yaml}---\n").into_bytes();

        let parsed = book_metadata_from_markdown_bytes(&bytes).unwrap();

        assert_eq!(parsed.book_id, "book-123");
        assert_eq!(parsed.name, "Field Notes");
    }

    #[test]
    fn local_cloud_book_root_reuses_matching_book_and_avoids_unrelated_folder() {
        let temp = tempdir().unwrap();
        let preferred = temp.path().join("Field-Notes");
        let mut existing = Book::create(&preferred, "Field Notes").unwrap();
        existing.metadata.book_id = "existing-book".to_string();
        existing.save_metadata().unwrap();
        let matching = BookMetadata {
            book_id: "existing-book".to_string(),
            name: "Field Notes".to_string(),
            ..BookMetadata::new("ignored")
        };
        let unrelated = BookMetadata {
            book_id: "other-book".to_string(),
            name: "Field Notes".to_string(),
            ..BookMetadata::new("ignored")
        };

        assert_eq!(
            local_cloud_book_root(temp.path(), &matching).unwrap(),
            preferred
        );
        assert_eq!(
            local_cloud_book_root(temp.path(), &unrelated).unwrap(),
            temp.path().join("Field-Notes-2")
        );
    }

    #[test]
    fn delete_cloud_book_prefix_cleans_all_matching_objects() {
        let mut store = syllepsis_core::sync::MemoryManagedObjectStore::default();
        store
            .put("syllepsis-sync/books/book-1/manifest.json", b"{}")
            .unwrap();
        store
            .put(
                "syllepsis-sync/books/book-1/notes/note-a/patches/1.loro_patch",
                b"x",
            )
            .unwrap();
        store
            .put(
                "syllepsis-sync/books/book-2/notes/note-b/patches/1.loro_patch",
                b"y",
            )
            .unwrap();

        let prefix = "syllepsis-sync/books/book-1/";
        let entries = store.list(prefix).unwrap();
        for entry in entries {
            store.delete(&entry.path).unwrap();
        }

        assert!(store
            .list("syllepsis-sync/books/book-1/")
            .unwrap()
            .is_empty());
        assert_eq!(store.list("syllepsis-sync/books/book-2/").unwrap().len(), 1);
    }
}
