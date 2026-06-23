//! Sync commands: mounted-folder sync, git snapshots, file-watch observability, and managed cloud
//! patch-log sync.

use std::path::Path;

use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::State;

use syllepsis_core::app::git as git_app;
use syllepsis_core::app::sync as app;
use syllepsis_core::app::sync::SyncStatusDto;
use syllepsis_core::storage::Book;
use syllepsis_core::sync::{
    list_activity, prune_activity, ManagedCloudReport, ManagedCloudSyncEngine, ManagedObjectEntry,
    ManagedObjectStore, SyncActivityEvent, SyncProviderDescriptor, SyncReport,
};

use crate::state::AppState;

const SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync";
const ACCESS_TOKEN_FIELD: &str = "access-token";
const REFRESH_TOKEN_FIELD: &str = "refresh-token";
const OAUTH_STATE_FIELD: &str = "oauth-state";
const CODE_VERIFIER_FIELD: &str = "code-verifier";
const OAUTH_REDIRECT_URI: &str = "syllepsis://oauth-callback";
const ACTIVITY_RETENTION_DAYS: i64 = 90;

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
            if should_ignore_watch_path(&watch_root, &path) {
                continue;
            }
            let rel = path
                .strip_prefix(&watch_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|p| p.replace('\\', "/"));
            let kind = if rel
                .as_deref()
                .is_some_and(|p| p.contains("conflict") || p.contains("Conflicted copy"))
            {
                "conflict_detected"
            } else {
                "external_update"
            };
            let _ = syllepsis_core::sync::append_activity(
                &watch_root,
                &SyncActivityEvent::new("file_watch", kind, rel, format!("{:?}", event.kind)),
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
pub fn connect_cloud_sync_provider(provider: String) -> Result<CloudSyncConnectStart, String> {
    descriptor_for(&provider)?;
    let state = ulid::Ulid::new().to_string();
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let mut store = KeyringSyncCredentialStore;
    store.set(&account(&provider, OAUTH_STATE_FIELD), &state)?;
    store.set(&account(&provider, CODE_VERIFIER_FIELD), &verifier)?;
    Ok(CloudSyncConnectStart {
        auth_url: oauth_url(&provider, &state, &challenge)?,
        redirect_uri: OAUTH_REDIRECT_URI.to_string(),
        provider,
        state,
    })
}

#[tauri::command]
pub fn handle_cloud_sync_oauth_callback(
    callback_url: String,
) -> Result<CloudSyncProviderStatus, String> {
    let params = parse_query_params(&callback_url);
    let provider = params
        .get("provider")
        .cloned()
        .unwrap_or_else(|| "google_drive".to_string());
    let descriptor = descriptor_for(&provider)?;
    let mut store = KeyringSyncCredentialStore;

    let expected_state = store
        .get(&account(&provider, OAUTH_STATE_FIELD))?
        .ok_or_else(|| "no pending OAuth request for this provider".to_string())?;
    let callback_state = params
        .get("state")
        .ok_or_else(|| "OAuth callback did not include state".to_string())?;
    if callback_state != &expected_state {
        return Err("OAuth callback state did not match the pending request".to_string());
    }
    store.delete(&account(&provider, OAUTH_STATE_FIELD))?;

    if let Some(token) = params.get("refresh_token").or_else(|| params.get("token")) {
        // Some providers (or manual testing) may deliver a token directly.
        store.set(&account(&provider, REFRESH_TOKEN_FIELD), token)?;
    } else if let Some(code) = params.get("code") {
        // Standard authorization-code + PKCE flow: exchange the code for tokens.
        let verifier = store
            .get(&account(&provider, CODE_VERIFIER_FIELD))?
            .ok_or_else(|| "no PKCE code verifier found; restart the connect flow".to_string())?;
        store.delete(&account(&provider, CODE_VERIFIER_FIELD))?;
        let credentials = exchange_code_for_tokens(&provider, code, &verifier)?;
        if let Some(access) = credentials.access_token {
            store.set(&account(&provider, ACCESS_TOKEN_FIELD), &access)?;
        }
        if let Some(refresh) = credentials.refresh_token {
            store.set(&account(&provider, REFRESH_TOKEN_FIELD), &refresh)?;
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
        let entry = keyring::Entry::new(SYNC_KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("read keychain entry: {e}")),
        }
    }

    fn set(&mut self, account: &str, secret: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SYNC_KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        entry
            .set_password(secret)
            .map_err(|e| format!("write keychain entry: {e}"))
    }

    fn delete(&mut self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SYNC_KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("delete keychain entry: {e}")),
        }
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
    access_token: Option<String>,
    refresh_token: Option<String>,
}

fn credentials_for(provider: &str) -> Result<CloudCredentials, String> {
    descriptor_for(provider)?;
    let store = KeyringSyncCredentialStore;
    let credentials = CloudCredentials {
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

fn oauth_url(provider: &str, state: &str, pkce_challenge: &str) -> Result<String, String> {
    let descriptor = descriptor_for(provider)?;
    let client_id_env = match provider {
        "google_drive" => "SYLLEPSIS_GOOGLE_DRIVE_CLIENT_ID",
        "dropbox" => "SYLLEPSIS_DROPBOX_CLIENT_ID",
        "onedrive" => "SYLLEPSIS_ONEDRIVE_CLIENT_ID",
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    let client_id = std::env::var(client_id_env)
        .map_err(|_| format!("{client_id_env} is not configured for OAuth"))?;
    let scope = match provider {
        "google_drive" => "https://www.googleapis.com/auth/drive.file",
        "dropbox" => "files.content.write files.content.read",
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
        percent_encode(OAUTH_REDIRECT_URI),
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
) -> Result<CloudCredentials, String> {
    let client_id_env = match provider {
        "google_drive" => "SYLLEPSIS_GOOGLE_DRIVE_CLIENT_ID",
        "dropbox" => "SYLLEPSIS_DROPBOX_CLIENT_ID",
        "onedrive" => "SYLLEPSIS_ONEDRIVE_CLIENT_ID",
        other => return Err(format!("unknown cloud sync provider: {other}")),
    };
    let client_id = std::env::var(client_id_env)
        .map_err(|_| format!("{client_id_env} is not configured for OAuth"))?;
    let endpoint = token_endpoint(provider)?;

    // Build an application/x-www-form-urlencoded body without the `form` reqwest feature.
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        percent_encode(code),
        percent_encode(OAUTH_REDIRECT_URI),
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
        access_token,
        refresh_token,
    })
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

fn should_ignore_watch_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|rel| rel.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|first| first == "_sync" || first == "_derived")
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
