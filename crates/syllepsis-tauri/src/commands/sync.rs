//! Sync commands: mounted-folder sync, git snapshots, file-watch observability, and managed cloud
//! patch-log sync.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use syllepsis_core::app::git as git_app;
use syllepsis_core::app::sync as app;
use syllepsis_core::app::sync::SyncStatusDto;
use syllepsis_core::id::NoteId;
use syllepsis_core::storage::{layout, Book, NoteStore};
use syllepsis_core::sync::{
    latest_note_activity, list_activity, prune_activity, summarize_activity, ManagedCloudReport,
    ManagedCloudSyncEngine, ManagedObjectEntry, ManagedObjectStore, NoteSyncActivity,
    SyncActivityEvent, SyncActivitySummary, SyncProviderDescriptor, SyncReport,
};

use crate::state::AppState;

const SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync";
const DEVELOPMENT_SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync.dev";
const ACCESS_TOKEN_FIELD: &str = "access-token";
const REFRESH_TOKEN_FIELD: &str = "refresh-token";
const OAUTH_STATE_FIELD: &str = "oauth-state";
const CODE_VERIFIER_FIELD: &str = "code-verifier";
const OAUTH_CALLBACK_PATH: &str = "/oauth-callback";
const OAUTH_CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const ACTIVITY_RETENTION_DAYS: i64 = 90;
const WATCH_ACTIVITY_DEBOUNCE: Duration = Duration::from_millis(750);

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
        app::sync_to_local_folder(book, &remote_path).map_err(|e| e.to_string())
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
    let root = {
        let guard = state.book.lock().unwrap();
        guard
            .as_ref()
            .map(|book| book.root.clone())
            .ok_or_else(|| "no book is open".to_string())?
    };
    let watch_root = root.clone();
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
    state: State<AppState>,
) -> Result<OperationalActivitySummary, String> {
    with_book!(state, book, {
        prune_activity(&book.root, ACTIVITY_RETENTION_DAYS).map_err(|e| e.to_string())?;
        let events = list_activity(&book.root).map_err(|e| e.to_string())?;
        let activity = summarize_activity(&events, Utc::now());
        let git = operational_git_summary(book);
        let cloud = operational_cloud_summary();
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

fn operational_cloud_summary() -> OperationalCloudSummary {
    match cloud_sync_provider_statuses() {
        Ok(statuses) => OperationalCloudSummary {
            provider_count: statuses.len(),
            connected_provider_count: statuses.iter().filter(|status| status.connected).count(),
            connected_provider_names: statuses
                .into_iter()
                .filter(|status| status.connected)
                .map(|status| status.display_name)
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
}

#[tauri::command]
pub fn cloud_sync_provider_descriptors() -> Vec<CloudSyncProviderDescriptor> {
    cloud_descriptors()
}

#[tauri::command]
pub fn cloud_sync_provider_statuses() -> Result<Vec<CloudSyncProviderStatus>, String> {
    let store = KeyringSyncCredentialStore;
    cloud_descriptors()
        .into_iter()
        .map(|descriptor| {
            Ok(CloudSyncProviderStatus {
                connected: token_for(&store, &descriptor.provider)?.is_some(),
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
) -> Result<CloudSyncProviderStatus, String> {
    let params = parse_query_params(&callback_url);
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

    if let Some(token) = params.get("refresh_token").or_else(|| params.get("token")) {
        // Some providers (or manual testing) may deliver a token directly.
        store.set(&account(provider, REFRESH_TOKEN_FIELD), token)?;
    } else if let Some(code) = params.get("code") {
        // Standard authorization-code + PKCE flow: exchange the code for tokens.
        let verifier = store
            .get(&account(provider, CODE_VERIFIER_FIELD))?
            .ok_or_else(|| "no PKCE code verifier found; restart the connect flow".to_string())?;
        store.delete(&account(provider, CODE_VERIFIER_FIELD))?;
        let credentials = exchange_code_for_tokens(provider, code, &verifier, redirect_uri)?;
        if let Some(access) = credentials.access_token {
            store.set(&account(provider, ACCESS_TOKEN_FIELD), &access)?;
        }
        if let Some(refresh) = credentials.refresh_token {
            store.set(&account(provider, REFRESH_TOKEN_FIELD), &refresh)?;
        }
    } else {
        return Err("OAuth callback did not include a token or code".to_string());
    }
    Ok(CloudSyncProviderStatus {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        connected: true,
        requires_loro: true,
    })
}

#[tauri::command]
pub fn disconnect_cloud_sync_provider(provider: String) -> Result<CloudSyncProviderStatus, String> {
    let descriptor = descriptor_for(&provider)?;
    let mut store = KeyringSyncCredentialStore;
    store.delete(&account(&provider, ACCESS_TOKEN_FIELD))?;
    store.delete(&account(&provider, REFRESH_TOKEN_FIELD))?;
    store.delete(&account(&provider, OAUTH_STATE_FIELD))?;
    store.delete(&account(&provider, CODE_VERIFIER_FIELD))?;
    Ok(CloudSyncProviderStatus {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        connected: false,
        requires_loro: true,
    })
}

#[tauri::command]
pub fn list_cloud_books(provider: String) -> Result<Vec<CloudBookSummary>, String> {
    let store = opendal_store_for(&provider)?;
    let entries = store
        .list("syllepsis-sync/books/")
        .map_err(|e| e.to_string())?;
    let mut summaries = Vec::new();
    for entry in entries {
        if !entry.path.ends_with("/manifest.json") && !entry.path.ends_with("manifest.json") {
            continue;
        }
        let bytes = store.get(&entry.path).map_err(|e| e.to_string())?;
        let manifest: syllepsis_core::sync::BookManifest =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        summaries.push(CloudBookSummary {
            book_id: manifest.book_id,
            name: manifest.name,
            updated_at: manifest.updated_at.to_rfc3339(),
        });
    }
    Ok(summaries)
}

#[tauri::command]
pub fn upload_book_to_cloud(
    state: State<AppState>,
    provider: String,
) -> Result<ManagedCloudReport, String> {
    with_book!(state, book, {
        let store = opendal_store_for(&provider)?;
        let mut engine = ManagedCloudSyncEngine::new(book, store, provider);
        engine.sync().map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn sync_managed_cloud_now(
    state: State<AppState>,
    provider: String,
) -> Result<ManagedCloudReport, String> {
    upload_book_to_cloud(state, provider)
}

#[tauri::command]
pub fn open_cloud_book(
    state: State<AppState>,
    provider: String,
    book_id: String,
    parent_path: String,
) -> Result<(), String> {
    let store = opendal_store_for(&provider)?;
    let manifest_path = format!("syllepsis-sync/books/{book_id}/manifest.json");
    let manifest: syllepsis_core::sync::BookManifest =
        serde_json::from_slice(&store.get(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let root = Path::new(&parent_path).join(safe_folder_name(&manifest.name));
    let mut book = if root.exists() {
        Book::open(&root).map_err(|e| e.to_string())?
    } else {
        Book::create(&root, &manifest.name).map_err(|e| e.to_string())?
    };
    book.metadata.book_id = manifest.book_id;
    book.save_metadata().map_err(|e| e.to_string())?;
    let mut engine = ManagedCloudSyncEngine::new(&book, store, provider);
    engine.sync().map_err(|e| e.to_string())?;
    *state.book.lock().unwrap() = Some(book);
    Ok(())
}

struct KeyringSyncCredentialStore;

impl KeyringSyncCredentialStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
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

struct OpenDalManagedObjectStore {
    op: opendal::blocking::Operator,
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

fn opendal_store_for(provider: &str) -> Result<OpenDalManagedObjectStore, String> {
    let credentials = credentials_for(provider)?;
    let op = match provider {
        "google_drive" => {
            let mut builder = opendal::services::Gdrive::default();
            builder = builder.root("/");
            apply_opendal_tokens_gdrive(builder, &credentials)?
        }
        "dropbox" => {
            let mut builder = opendal::services::Dropbox::default();
            builder = builder.root("/");
            apply_opendal_tokens_dropbox(builder, &credentials)?
        }
        "onedrive" => {
            let mut builder = opendal::services::Onedrive::default();
            builder = builder.root("/");
            apply_opendal_tokens_onedrive(builder, &credentials)?
        }
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    opendal::blocking::Operator::new(op)
        .map(|op| OpenDalManagedObjectStore { op })
        .map_err(|e| format!("create blocking OpenDAL operator: {e}"))
}

struct CloudCredentials {
    client_id: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

fn credentials_for(provider: &str) -> Result<CloudCredentials, String> {
    descriptor_for(provider)?;
    let store = KeyringSyncCredentialStore;
    let credentials = CloudCredentials {
        client_id: require_oauth_client_id(provider)?,
        access_token: token_for_field(&store, provider, ACCESS_TOKEN_FIELD)?,
        refresh_token: token_for_field(&store, provider, REFRESH_TOKEN_FIELD)?,
    };
    if credentials.access_token.is_none() && credentials.refresh_token.is_none() {
        return Err(format!("{provider} is not connected"));
    }
    Ok(credentials)
}

fn token_for(store: &KeyringSyncCredentialStore, provider: &str) -> Result<Option<String>, String> {
    Ok(
        token_for_field(store, provider, REFRESH_TOKEN_FIELD)?.or(token_for_field(
            store,
            provider,
            ACCESS_TOKEN_FIELD,
        )?),
    )
}

fn token_for_field(
    store: &KeyringSyncCredentialStore,
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
    }
    if let Some(token) = &credentials.refresh_token {
        builder = builder.refresh_token(token);
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
}

#[derive(Deserialize)]
struct CloudSyncOAuthClientIds {
    google_drive: CloudSyncOAuthClientConfig,
    dropbox: CloudSyncOAuthClientConfig,
    onedrive: CloudSyncOAuthClientConfig,
}

fn oauth_client_config(provider: &str) -> Result<CloudSyncOAuthClientConfig, String> {
    let configured_client_ids: CloudSyncOAuthClientIds =
        serde_json::from_str(include_str!("../../oauth-client-ids.json"))
            .map_err(|error| format!("parse bundled OAuth client IDs: {error}"))?;
    let config = match provider {
        "google_drive" => configured_client_ids.google_drive,
        "dropbox" => configured_client_ids.dropbox,
        "onedrive" => configured_client_ids.onedrive,
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    if config.client_id.trim().is_empty() {
        return Err(format!(
            "{provider} OAuth is not configured in this build; add the Syllepsis app client ID to crates/syllepsis-tauri/oauth-client-ids.json"
        ));
    }
    Ok(config)
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

/// Exchange an OAuth authorization code for access/refresh tokens using PKCE (no client secret
/// required for public/desktop app registrations).
fn exchange_code_for_tokens(
    provider: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<CloudCredentials, String> {
    let client_id = require_oauth_client_id(provider)?;
    let endpoint = token_endpoint(provider)?;

    // Build an application/x-www-form-urlencoded body without the `form` reqwest feature.
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        percent_encode(code),
        percent_encode(redirect_uri),
        percent_encode(&client_id),
        percent_encode(verifier),
    );
    let response = reqwest::blocking::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|e| format!("token exchange request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("token exchange returned {status}: {body}"));
    }
    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("token exchange response parse failed: {e}"))?;

    let access_token = json
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
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
        access_token,
        refresh_token,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
