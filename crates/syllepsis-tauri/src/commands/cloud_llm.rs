//! Device-local cloud LLM credential commands.
//!
//! API keys must never be written to book config or markdown. The desktop shell stores them in
//! the OS keychain and exposes only boolean status to the UI. Provider execution also happens in
//! the shell so secrets do not cross the IPC boundary.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};
use tauri::State;

use syllepsis_core::app::llm::{self as app, CloudLlmCompletion, CloudLlmPrompt};
use syllepsis_core::config::ModelRef;
use syllepsis_core::llm::prompts::LlmTaskOptions;
use syllepsis_core::llm::{LlmTask, Proposal};

use crate::secrets::{self, KeyringVaultStore, LlmSecret, VaultStore};
use crate::state::{AppState, CachedCloudLlmCredentials, CachedCloudLlmModels};

const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 2048;
const CONNECTION_TEST_TIMEOUT_SECONDS: u64 = 15;
const MODEL_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmProviderDescriptor {
    pub provider: String,
    pub display_name: String,
    pub base_url_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmProviderSettings {
    pub provider: String,
    /// `None` leaves the existing key unchanged; an empty string clears it.
    pub api_key: Option<String>,
    /// `None` leaves the existing base URL unchanged; an empty string clears it.
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmModel {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmConnectionTestResult {
    pub provider: String,
    pub display_name: String,
    pub model_count: usize,
    pub models: Vec<CloudLlmModel>,
    pub authentication_status: CloudLlmAuthenticationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudLlmAuthenticationStatus {
    Verified,
    NotRequired,
    NotTested,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CloudLlmCredentials {
    api_key: Option<String>,
    base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CloudLlmConnectionTestRequest {
    url: String,
    headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloudLlmHttpRequest {
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

/// Built-in cloud provider descriptors known to the management UI.
#[tauri::command]
pub fn cloud_llm_provider_descriptors() -> Vec<CloudLlmProviderDescriptor> {
    provider_descriptors()
}

/// Save or clear provider credentials in the OS keychain.
#[tauri::command]
pub fn save_cloud_llm_provider_settings(
    state: State<AppState>,
    settings: CloudLlmProviderSettings,
) -> Result<(), String> {
    {
        let _guard = state.secrets_lock.lock().unwrap();
        let mut store = KeyringVaultStore::new();
        save_settings(&mut store, settings.clone())?;
    }
    merge_cached_credentials(&state, settings);
    Ok(())
}

/// Clear all credential fields for a provider.
#[tauri::command]
pub fn clear_cloud_llm_provider_settings(
    state: State<AppState>,
    provider: String,
) -> Result<(), String> {
    {
        let _guard = state.secrets_lock.lock().unwrap();
        let mut store = KeyringVaultStore::new();
        clear_settings(&mut store, &provider)?;
    }
    state
        .cloud_llm_credentials
        .lock()
        .unwrap()
        .remove(&provider);
    state.cloud_llm_models.lock().unwrap().remove(&provider);
    Ok(())
}

/// Validate draft or stored credentials with a model-list request that consumes no LLM tokens.
#[tauri::command]
pub fn test_cloud_llm_provider_connection(
    state: State<AppState>,
    settings: CloudLlmProviderSettings,
) -> Result<CloudLlmConnectionTestResult, String> {
    let mut store = KeyringVaultStore::new();
    let result = test_connection(&state, &mut store, settings)?;
    cache_cloud_models(&state, &result.provider, &result.models);
    Ok(result)
}

/// Return cached provider models or refresh them from stored credentials when stale.
#[tauri::command]
pub fn list_cloud_llm_provider_models(
    state: State<AppState>,
    provider: String,
) -> Result<Vec<CloudLlmModel>, String> {
    descriptor_for(&provider)?;
    if let Some(models) = cached_cloud_models(&state, &provider) {
        return Ok(models);
    }
    let mut store = KeyringVaultStore::new();
    let models = list_provider_models(&state, &mut store, &provider)?;
    cache_cloud_models(&state, &provider, &models);
    Ok(models)
}

pub(crate) fn cloud_provider_is_configured(
    state: &AppState,
    provider: &str,
) -> Result<bool, String> {
    let mut store = KeyringVaultStore::new();
    provider_is_configured(state, &mut store, provider)
}

/// Generate a proposal through a configured cloud or OpenAI-compatible local server.
#[tauri::command]
pub fn generate_cloud_proposal(
    state: State<AppState>,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> Result<Proposal, String> {
    generate_cloud_proposal_for_state(
        &state,
        note_id,
        task,
        model_override,
        &LlmTaskOptions::default(),
    )
}

pub(crate) fn generate_cloud_proposal_for_state(
    state: &AppState,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
    options: &LlmTaskOptions,
) -> Result<Proposal, String> {
    let prompt = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        app::prepare_cloud_prompt_with_options(book, &note_id, task, model_override, options)
            .map_err(|e| e.to_string())?
    };
    let mut store = KeyringVaultStore::new();
    let content = execute_cloud_prompt(state, &mut store, &prompt)?;
    proposal_from_completed_prompt(state, prompt, content)
}

fn provider_descriptors() -> Vec<CloudLlmProviderDescriptor> {
    vec![
        CloudLlmProviderDescriptor {
            provider: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            base_url_required: false,
        },
        CloudLlmProviderDescriptor {
            provider: "openai_compatible".to_string(),
            display_name: "OpenAI-compatible".to_string(),
            base_url_required: true,
        },
    ]
}

fn save_settings(
    store: &mut impl VaultStore,
    settings: CloudLlmProviderSettings,
) -> Result<(), String> {
    descriptor_for(&settings.provider)?;
    let mut secret = secrets::read_llm_secret(store, &settings.provider)?.unwrap_or_default();
    apply_optional_field(&mut secret.api_key, settings.api_key);
    apply_optional_field(&mut secret.base_url, settings.base_url);
    secrets::write_llm_secret(store, &settings.provider, secret)
}

fn clear_settings(store: &mut impl VaultStore, provider: &str) -> Result<(), String> {
    descriptor_for(provider)?;
    secrets::delete_llm_secret(store, provider)
}

/// Apply an optional settings field to a stored secret: `None` leaves it untouched, an empty string
/// clears it, and any other value replaces it (trimmed).
fn apply_optional_field(field: &mut Option<String>, update: Option<String>) {
    match update {
        None => {}
        Some(value) if value.trim().is_empty() => *field = None,
        Some(value) => *field = Some(value.trim().to_string()),
    }
}

fn descriptor_for(provider: &str) -> Result<CloudLlmProviderDescriptor, String> {
    provider_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.provider == provider)
        .ok_or_else(|| format!("unknown cloud LLM provider: {provider}"))
}

fn test_connection(
    state: &AppState,
    store: &mut impl VaultStore,
    settings: CloudLlmProviderSettings,
) -> Result<CloudLlmConnectionTestResult, String> {
    let descriptor = descriptor_for(&settings.provider)?;
    let credentials = credentials_with_draft_overrides(state, store, &settings)?;
    cache_credentials(state, &settings.provider, &credentials);
    let request = build_connection_test_request(&settings.provider, &credentials)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(
            CONNECTION_TEST_TIMEOUT_SECONDS,
        ))
        .build()
        .map_err(|e| format!("create LLM connection-test client: {e}"))?;
    let authenticated_response = send_connection_test_request(&client, &request)
        .map_err(|e| format!("connect to {}: {e}", descriptor.display_name))?;
    if !authenticated_response.status.is_success() {
        return Err(format!(
            "{} returned HTTP {}: {}",
            descriptor.display_name,
            authenticated_response.status.as_u16(),
            truncate_for_error(&authenticated_response.body)
        ));
    }
    let json: Value = serde_json::from_str(&authenticated_response.body).map_err(|e| {
        format!(
            "parse {} model-list response JSON: {e}",
            descriptor.display_name
        )
    })?;
    let models = parse_models(&descriptor.display_name, &json)?;
    let authentication_status = determine_authentication_status(&client, &request, &credentials);

    Ok(CloudLlmConnectionTestResult {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        model_count: models.len(),
        models,
        authentication_status,
    })
}

fn list_provider_models(
    state: &AppState,
    store: &mut impl VaultStore,
    provider: &str,
) -> Result<Vec<CloudLlmModel>, String> {
    let descriptor = descriptor_for(provider)?;
    let credentials = credentials_for(state, store, provider)?;
    let request = build_connection_test_request(provider, &credentials)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(
            CONNECTION_TEST_TIMEOUT_SECONDS,
        ))
        .build()
        .map_err(|e| format!("create LLM model-list client: {e}"))?;
    let response = send_connection_test_request(&client, &request)
        .map_err(|e| format!("connect to {}: {e}", descriptor.display_name))?;
    if !response.status.is_success() {
        return Err(format!(
            "{} returned HTTP {}: {}",
            descriptor.display_name,
            response.status.as_u16(),
            truncate_for_error(&response.body)
        ));
    }
    let json: Value = serde_json::from_str(&response.body).map_err(|e| {
        format!(
            "parse {} model-list response JSON: {e}",
            descriptor.display_name
        )
    })?;
    parse_models(&descriptor.display_name, &json)
}

fn credentials_with_draft_overrides(
    state: &AppState,
    store: &mut impl VaultStore,
    settings: &CloudLlmProviderSettings,
) -> Result<CloudLlmCredentials, String> {
    descriptor_for(&settings.provider)?;
    let cached = cached_credentials(state, &settings.provider);
    // Fall back to the vault only for fields the draft and cache leave unspecified.
    let need_stored = (settings.api_key.is_none()
        && cached
            .as_ref()
            .map(|credentials| credentials.api_key.is_none())
            .unwrap_or(true))
        || (settings.base_url.is_none()
            && cached
                .as_ref()
                .map(|credentials| credentials.base_url.is_none())
                .unwrap_or(true));
    let stored = if need_stored {
        let _guard = state.secrets_lock.lock().unwrap();
        secrets::read_llm_secret(store, &settings.provider)?.unwrap_or_default()
    } else {
        LlmSecret::default()
    };
    Ok(CloudLlmCredentials {
        api_key: credential_field_with_draft_override(
            settings.api_key.as_ref(),
            cached
                .as_ref()
                .and_then(|credentials| credentials.api_key.clone()),
            stored.api_key,
        ),
        base_url: credential_field_with_draft_override(
            settings.base_url.as_ref(),
            cached
                .as_ref()
                .and_then(|credentials| credentials.base_url.clone()),
            stored.base_url,
        ),
    })
}

fn credential_field_with_draft_override(
    draft_value: Option<&String>,
    cached_value: Option<String>,
    stored_value: Option<String>,
) -> Option<String> {
    match draft_value {
        Some(value) => trimmed_secret(Some(value.clone())),
        None => match cached_value {
            Some(value) => trimmed_secret(Some(value)),
            None => trimmed_secret(stored_value),
        },
    }
}

fn build_connection_test_request(
    provider: &str,
    credentials: &CloudLlmCredentials,
) -> Result<CloudLlmConnectionTestRequest, String> {
    match provider {
        "anthropic" => {
            let api_key = credentials
                .api_key
                .as_ref()
                .ok_or_else(|| "Anthropic API key is not configured".to_string())?;
            Ok(CloudLlmConnectionTestRequest {
                url: ANTHROPIC_MODELS_URL.to_string(),
                headers: vec![
                    ("x-api-key".to_string(), api_key.clone()),
                    (
                        "anthropic-version".to_string(),
                        ANTHROPIC_VERSION.to_string(),
                    ),
                ],
            })
        }
        "openai_compatible" => {
            let base_url = credentials
                .base_url
                .as_ref()
                .ok_or_else(|| "OpenAI-compatible base URL is not configured".to_string())?;
            let mut headers = Vec::new();
            if let Some(api_key) = &credentials.api_key {
                headers.push(("authorization".to_string(), format!("Bearer {api_key}")));
            }
            Ok(CloudLlmConnectionTestRequest {
                url: openai_models_url(base_url)?,
                headers,
            })
        }
        provider => Err(format!("unknown cloud LLM provider: {provider}")),
    }
}

fn parse_models(display_name: &str, response: &Value) -> Result<Vec<CloudLlmModel>, String> {
    let data = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!("{display_name} model-list response did not include a data array")
        })?;
    Ok(data
        .iter()
        .filter_map(|model| {
            model
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| CloudLlmModel { id: id.to_string() })
        })
        .collect())
}

fn cache_cloud_models(state: &AppState, provider: &str, models: &[CloudLlmModel]) {
    state.cloud_llm_models.lock().unwrap().insert(
        provider.to_string(),
        CachedCloudLlmModels {
            model_ids: models.iter().map(|model| model.id.clone()).collect(),
            fetched_at: SystemTime::now(),
        },
    );
}

fn cached_cloud_models(state: &AppState, provider: &str) -> Option<Vec<CloudLlmModel>> {
    let cache = state.cloud_llm_models.lock().unwrap();
    let entry = cache.get(provider)?;
    if entry.fetched_at.elapsed().ok()? > MODEL_CACHE_TTL {
        return None;
    }
    Some(
        entry
            .model_ids
            .iter()
            .map(|id| CloudLlmModel { id: id.clone() })
            .collect(),
    )
}

struct CloudLlmConnectionTestResponse {
    status: reqwest::StatusCode,
    body: String,
}

fn send_connection_test_request(
    client: &Client,
    request: &CloudLlmConnectionTestRequest,
) -> Result<CloudLlmConnectionTestResponse, String> {
    let mut builder = client.get(&request.url);
    for (header, value) in &request.headers {
        builder = builder.header(header, value);
    }
    let response = builder.send().map_err(|e| e.to_string())?;
    let status = response.status();
    let body = response.text().map_err(|e| e.to_string())?;
    Ok(CloudLlmConnectionTestResponse { status, body })
}

fn determine_authentication_status(
    client: &Client,
    authenticated_request: &CloudLlmConnectionTestRequest,
    credentials: &CloudLlmCredentials,
) -> CloudLlmAuthenticationStatus {
    if credentials.api_key.is_none() {
        return CloudLlmAuthenticationStatus::NotTested;
    }
    let unauthenticated_request = CloudLlmConnectionTestRequest {
        url: authenticated_request.url.clone(),
        headers: authenticated_request
            .headers
            .iter()
            .filter(|(header, _)| header != "authorization" && header != "x-api-key")
            .cloned()
            .collect(),
    };
    match send_connection_test_request(client, &unauthenticated_request) {
        Ok(response) => classify_unauthenticated_response(response.status, &response.body),
        Err(_) => CloudLlmAuthenticationStatus::Inconclusive,
    }
}

fn classify_unauthenticated_response(
    status: reqwest::StatusCode,
    body: &str,
) -> CloudLlmAuthenticationStatus {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return CloudLlmAuthenticationStatus::Verified;
    }
    if status.is_success()
        && serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|json| json.get("data").and_then(Value::as_array).cloned())
            .is_some()
    {
        return CloudLlmAuthenticationStatus::NotRequired;
    }
    CloudLlmAuthenticationStatus::Inconclusive
}

fn execute_cloud_prompt(
    state: &AppState,
    store: &mut impl VaultStore,
    prompt: &CloudLlmPrompt,
) -> Result<String, String> {
    let credentials = credentials_for(state, store, &prompt.provider)?;
    let request = build_provider_request(prompt, &credentials)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("create LLM HTTP client: {e}"))?;
    let mut builder = client.post(&request.url).json(&request.body);
    for (header, value) in &request.headers {
        builder = builder.header(header, value);
    }
    let response = builder
        .send()
        .map_err(|e| format!("call {} LLM provider: {e}", prompt.provider))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("read {} LLM response: {e}", prompt.provider))?;
    if !status.is_success() {
        return Err(format!(
            "{} LLM provider returned HTTP {}: {}",
            prompt.provider,
            status.as_u16(),
            truncate_for_error(&body)
        ));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|e| format!("parse {} LLM response JSON: {e}", prompt.provider))?;
    parse_provider_response(&prompt.provider, &json)
}

fn proposal_from_completed_prompt(
    state: &AppState,
    prompt: CloudLlmPrompt,
    content: String,
) -> Result<Proposal, String> {
    let guard = state.book.lock().unwrap();
    let book = guard
        .as_ref()
        .ok_or_else(|| "no book is open".to_string())?;
    app::proposal_from_cloud_completion(book, completion_from_prompt(prompt, content))
        .map_err(|e| e.to_string())
}

fn completion_from_prompt(prompt: CloudLlmPrompt, content: String) -> CloudLlmCompletion {
    CloudLlmCompletion {
        target_note_id: prompt.target_note_id,
        task: prompt.task,
        provider: prompt.provider,
        model: prompt.model,
        content,
    }
}

fn credentials_for(
    state: &AppState,
    store: &mut impl VaultStore,
    provider: &str,
) -> Result<CloudLlmCredentials, String> {
    descriptor_for(provider)?;
    if let Some(credentials) = cached_credentials(state, provider) {
        return Ok(credentials);
    }
    // Cold cache: read the single vault item under the shared lock, then cache it in memory.
    let secret = {
        let _guard = state.secrets_lock.lock().unwrap();
        secrets::read_llm_secret(store, provider)?.unwrap_or_default()
    };
    let credentials = CloudLlmCredentials {
        api_key: trimmed_secret(secret.api_key),
        base_url: trimmed_secret(secret.base_url),
    };
    cache_credentials(state, provider, &credentials);
    Ok(credentials)
}

fn provider_is_configured(
    state: &AppState,
    store: &mut impl VaultStore,
    provider: &str,
) -> Result<bool, String> {
    descriptor_for(provider)?;
    if let Some(credentials) = cached_credentials(state, provider) {
        return Ok(match provider {
            "anthropic" => credentials.api_key.is_some(),
            "openai_compatible" => credentials.base_url.is_some(),
            provider => return Err(format!("unknown cloud LLM provider: {provider}")),
        });
    }
    let secret = {
        let _guard = state.secrets_lock.lock().unwrap();
        secrets::read_llm_secret(store, provider)?.unwrap_or_default()
    };
    match provider {
        "anthropic" => {
            let api_key = trimmed_secret(secret.api_key);
            let configured = api_key.is_some();
            if configured {
                cache_credentials(
                    state,
                    provider,
                    &CloudLlmCredentials {
                        api_key,
                        base_url: None,
                    },
                );
            }
            Ok(configured)
        }
        "openai_compatible" => {
            let base_url = trimmed_secret(secret.base_url);
            let configured = base_url.is_some();
            if configured {
                let api_key = trimmed_secret(secret.api_key);
                cache_credentials(state, provider, &CloudLlmCredentials { api_key, base_url });
            }
            Ok(configured)
        }
        provider => Err(format!("unknown cloud LLM provider: {provider}")),
    }
}

fn cached_credentials(state: &AppState, provider: &str) -> Option<CloudLlmCredentials> {
    let cache = state.cloud_llm_credentials.lock().unwrap();
    let credentials = cache.get(provider)?;
    Some(CloudLlmCredentials {
        api_key: credentials.api_key.clone(),
        base_url: credentials.base_url.clone(),
    })
}

fn cache_credentials(state: &AppState, provider: &str, credentials: &CloudLlmCredentials) {
    state.cloud_llm_credentials.lock().unwrap().insert(
        provider.to_string(),
        CachedCloudLlmCredentials {
            api_key: credentials.api_key.clone(),
            base_url: credentials.base_url.clone(),
        },
    );
}

fn merge_cached_credentials(state: &AppState, settings: CloudLlmProviderSettings) {
    let mut cache = state.cloud_llm_credentials.lock().unwrap();
    let entry = cache
        .entry(settings.provider)
        .or_insert_with(|| CachedCloudLlmCredentials {
            api_key: None,
            base_url: None,
        });
    if let Some(api_key) = settings.api_key {
        entry.api_key = trimmed_secret(Some(api_key));
    }
    if let Some(base_url) = settings.base_url {
        entry.base_url = trimmed_secret(Some(base_url));
    }
}

fn trimmed_secret(secret: Option<String>) -> Option<String> {
    secret
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_provider_request(
    prompt: &CloudLlmPrompt,
    credentials: &CloudLlmCredentials,
) -> Result<CloudLlmHttpRequest, String> {
    match prompt.provider.as_str() {
        "anthropic" => build_anthropic_request(prompt, credentials),
        "openai_compatible" => build_openai_compatible_request(prompt, credentials),
        provider => Err(format!("unknown cloud LLM provider: {provider}")),
    }
}

fn build_anthropic_request(
    prompt: &CloudLlmPrompt,
    credentials: &CloudLlmCredentials,
) -> Result<CloudLlmHttpRequest, String> {
    let api_key = credentials
        .api_key
        .as_ref()
        .ok_or_else(|| "anthropic API key is not configured".to_string())?;
    Ok(CloudLlmHttpRequest {
        url: ANTHROPIC_MESSAGES_URL.to_string(),
        headers: vec![
            ("x-api-key".to_string(), api_key.clone()),
            (
                "anthropic-version".to_string(),
                ANTHROPIC_VERSION.to_string(),
            ),
        ],
        body: json!({
            "model": prompt.model,
            "system": prompt.system,
            "max_tokens": DEFAULT_MAX_TOKENS,
            "messages": [
                { "role": "user", "content": prompt.user }
            ]
        }),
    })
}

fn build_openai_compatible_request(
    prompt: &CloudLlmPrompt,
    credentials: &CloudLlmCredentials,
) -> Result<CloudLlmHttpRequest, String> {
    let base_url = credentials
        .base_url
        .as_ref()
        .ok_or_else(|| "OpenAI-compatible base URL is not configured".to_string())?;
    let mut headers = Vec::new();
    if let Some(api_key) = &credentials.api_key {
        headers.push(("authorization".to_string(), format!("Bearer {api_key}")));
    }
    Ok(CloudLlmHttpRequest {
        url: openai_chat_completions_url(base_url)?,
        headers,
        body: json!({
            "model": prompt.model,
            "messages": [
                { "role": "system", "content": prompt.system },
                { "role": "user", "content": prompt.user }
            ]
        }),
    })
}

fn openai_chat_completions_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("OpenAI-compatible base URL is not configured".to_string());
    }
    if trimmed.ends_with("/chat/completions") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/chat/completions"))
    }
}

fn openai_models_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("OpenAI-compatible base URL is not configured".to_string());
    }
    if let Some(api_root) = trimmed.strip_suffix("/chat/completions") {
        Ok(format!("{api_root}/models"))
    } else if trimmed.ends_with("/models") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/models"))
    }
}

fn parse_provider_response(provider: &str, response: &Value) -> Result<String, String> {
    match provider {
        "anthropic" => parse_anthropic_response(response),
        "openai_compatible" => parse_openai_compatible_response(response),
        provider => Err(format!("unknown cloud LLM provider: {provider}")),
    }
}

fn parse_anthropic_response(response: &Value) -> Result<String, String> {
    response
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
        })
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "Anthropic response did not include text content".to_string())
}

fn parse_openai_compatible_response(response: &Value) -> Result<String, String> {
    response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "OpenAI-compatible response did not include message content".to_string())
}

fn truncate_for_error(body: &str) -> String {
    const LIMIT: usize = 500;
    if body.len() <= LIMIT {
        body.to_string()
    } else {
        format!("{}...", &body[..LIMIT])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::test_support::MemoryVaultStore;

    #[test]
    fn save_settings_trims_and_preserves_unspecified_fields() {
        let mut store = MemoryVaultStore::default();
        save_settings(
            &mut store,
            CloudLlmProviderSettings {
                provider: "openai_compatible".to_string(),
                api_key: Some("  key  ".to_string()),
                base_url: Some(" http://localhost:8080/v1 ".to_string()),
            },
        )
        .unwrap();

        save_settings(
            &mut store,
            CloudLlmProviderSettings {
                provider: "openai_compatible".to_string(),
                api_key: None,
                base_url: Some(String::new()),
            },
        )
        .unwrap();

        let secret = secrets::read_llm_secret(&mut store, "openai_compatible")
            .unwrap()
            .unwrap();
        // The api_key is preserved (untouched by the second save), the base_url is cleared.
        assert_eq!(secret.api_key.as_deref(), Some("key"));
        assert_eq!(secret.base_url, None);
    }

    #[test]
    fn clear_settings_removes_all_provider_fields() {
        let mut store = MemoryVaultStore::default();
        secrets::write_llm_secret(
            &mut store,
            "anthropic",
            LlmSecret {
                api_key: Some("sk-secret".to_string()),
                base_url: None,
            },
        )
        .unwrap();

        clear_settings(&mut store, "anthropic").unwrap();

        assert!(secrets::read_llm_secret(&mut store, "anthropic")
            .unwrap()
            .is_none());
    }

    #[test]
    fn unknown_provider_is_rejected() {
        let mut store = MemoryVaultStore::default();
        let err = save_settings(
            &mut store,
            CloudLlmProviderSettings {
                provider: "not-real".to_string(),
                api_key: Some("key".to_string()),
                base_url: None,
            },
        )
        .unwrap_err();

        assert!(err.contains("unknown cloud LLM provider"));
    }

    #[test]
    fn provider_readiness_matches_required_credential_fields() {
        let state = AppState::new();
        let mut store = MemoryVaultStore::default();
        assert!(!provider_is_configured(&state, &mut store, "anthropic").unwrap());
        assert!(!provider_is_configured(&state, &mut store, "openai_compatible").unwrap());

        secrets::write_llm_secret(
            &mut store,
            "anthropic",
            LlmSecret {
                api_key: Some("sk-secret".to_string()),
                base_url: None,
            },
        )
        .unwrap();
        secrets::write_llm_secret(
            &mut store,
            "openai_compatible",
            LlmSecret {
                api_key: None,
                base_url: Some("http://localhost:8080/v1".to_string()),
            },
        )
        .unwrap();

        assert!(provider_is_configured(&state, &mut store, "anthropic").unwrap());
        assert!(provider_is_configured(&state, &mut store, "openai_compatible").unwrap());
    }

    #[test]
    fn model_parser_extracts_ids_and_ignores_malformed_items() {
        let models = parse_models(
            "OpenAI-compatible",
            &json!({
                "data": [
                    { "id": "gpt-5.4-mini" },
                    { "object": "model" },
                    { "id": "  claude-sonnet-4-6  " },
                    { "id": "" }
                ]
            }),
        )
        .unwrap();

        assert_eq!(
            models,
            vec![
                CloudLlmModel {
                    id: "gpt-5.4-mini".to_string()
                },
                CloudLlmModel {
                    id: "claude-sonnet-4-6".to_string()
                }
            ]
        );
    }

    #[test]
    fn model_parser_rejects_missing_data_array() {
        let err = parse_models("OpenAI-compatible", &json!({ "object": "list" })).unwrap_err();

        assert!(err.contains("did not include a data array"));
    }

    #[test]
    fn completed_prompt_preserves_route_metadata_for_proposal_wrapping() {
        let completion = completion_from_prompt(prompt("openai_compatible"), "done".to_string());

        assert_eq!(completion.target_note_id, "note-1");
        assert_eq!(completion.task, LlmTask::Summarize);
        assert_eq!(completion.provider, "openai_compatible");
        assert_eq!(completion.model, "model-a");
        assert_eq!(completion.content, "done");
    }

    fn prompt(provider: &str) -> CloudLlmPrompt {
        CloudLlmPrompt {
            target_note_id: "note-1".to_string(),
            task: LlmTask::Summarize,
            provider: provider.to_string(),
            model: "model-a".to_string(),
            system: "Follow the contract.".to_string(),
            user: "Summarize this note.".to_string(),
            output_contract: "plain_text_summary".to_string(),
        }
    }

    #[test]
    fn anthropic_request_requires_key_and_never_uses_base_url() {
        let credentials = CloudLlmCredentials {
            api_key: Some("sk-ant".to_string()),
            base_url: Some("http://localhost:8080/v1".to_string()),
        };

        let request = build_provider_request(&prompt("anthropic"), &credentials).unwrap();

        assert_eq!(request.url, ANTHROPIC_MESSAGES_URL);
        assert!(request
            .headers
            .contains(&("x-api-key".to_string(), "sk-ant".to_string())));
        assert_eq!(request.body["model"], "model-a");
        assert_eq!(
            request.body["messages"][0]["content"],
            "Summarize this note."
        );

        let err = build_provider_request(
            &prompt("anthropic"),
            &CloudLlmCredentials {
                api_key: None,
                base_url: None,
            },
        )
        .unwrap_err();
        assert!(err.contains("API key is not configured"));
    }

    #[test]
    fn openai_compatible_request_supports_local_servers_without_api_key() {
        let credentials = CloudLlmCredentials {
            api_key: None,
            base_url: Some(" http://localhost:8080/v1/ ".to_string()),
        };

        let request = build_provider_request(&prompt("openai_compatible"), &credentials).unwrap();

        assert_eq!(request.url, "http://localhost:8080/v1/chat/completions");
        assert!(request.headers.is_empty());
        assert_eq!(request.body["model"], "model-a");
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][1]["role"], "user");
    }

    #[test]
    fn openai_compatible_request_adds_bearer_header_when_key_exists() {
        let credentials = CloudLlmCredentials {
            api_key: Some("sk-openai".to_string()),
            base_url: Some("https://example.test/v1".to_string()),
        };

        let request = build_provider_request(&prompt("openai_compatible"), &credentials).unwrap();

        assert!(request
            .headers
            .contains(&("authorization".to_string(), "Bearer sk-openai".to_string())));
    }

    #[test]
    fn connection_test_uses_draft_values_without_mutating_stored_credentials() {
        let state = AppState::new();
        let mut store = MemoryVaultStore::default();
        secrets::write_llm_secret(
            &mut store,
            "openai_compatible",
            LlmSecret {
                api_key: None,
                base_url: Some("https://stored.example/v1".to_string()),
            },
        )
        .unwrap();

        let credentials = credentials_with_draft_overrides(
            &state,
            &mut store,
            &CloudLlmProviderSettings {
                provider: "openai_compatible".to_string(),
                api_key: Some(" draft-key ".to_string()),
                base_url: Some(" https://draft.example/v1 ".to_string()),
            },
        )
        .unwrap();

        assert_eq!(credentials.api_key.as_deref(), Some("draft-key"));
        assert_eq!(
            credentials.base_url.as_deref(),
            Some("https://draft.example/v1")
        );
        let stored = secrets::read_llm_secret(&mut store, "openai_compatible")
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.base_url.as_deref(),
            Some("https://stored.example/v1")
        );
    }

    #[test]
    fn connection_test_requests_model_endpoints_without_generation_bodies() {
        let anthropic = build_connection_test_request(
            "anthropic",
            &CloudLlmCredentials {
                api_key: Some("sk-ant".to_string()),
                base_url: None,
            },
        )
        .unwrap();
        let openai = build_connection_test_request(
            "openai_compatible",
            &CloudLlmCredentials {
                api_key: Some("sk-openai".to_string()),
                base_url: Some("https://example.test/v1/chat/completions".to_string()),
            },
        )
        .unwrap();

        assert_eq!(anthropic.url, ANTHROPIC_MODELS_URL);
        assert_eq!(openai.url, "https://example.test/v1/models");
        assert!(openai
            .headers
            .contains(&("authorization".to_string(), "Bearer sk-openai".to_string())));
    }

    #[test]
    fn connection_test_requires_a_model_list_response() {
        assert_eq!(
            parse_models(
                "Provider",
                &json!({ "data": [{ "id": "a" }, { "id": "b" }] })
            )
            .unwrap(),
            vec![
                CloudLlmModel {
                    id: "a".to_string()
                },
                CloudLlmModel {
                    id: "b".to_string()
                }
            ]
        );
        assert!(parse_models("Provider", &json!({ "models": [] }))
            .unwrap_err()
            .contains("did not include a data array"));
    }

    #[test]
    fn authentication_status_distinguishes_protected_and_public_model_lists() {
        assert_eq!(
            classify_unauthenticated_response(reqwest::StatusCode::UNAUTHORIZED, ""),
            CloudLlmAuthenticationStatus::Verified
        );
        assert_eq!(
            classify_unauthenticated_response(
                reqwest::StatusCode::OK,
                r#"{"data":[{"id":"model-a"}]}"#
            ),
            CloudLlmAuthenticationStatus::NotRequired
        );
        assert_eq!(
            classify_unauthenticated_response(reqwest::StatusCode::OK, "<html>sign in</html>"),
            CloudLlmAuthenticationStatus::Inconclusive
        );
    }

    #[test]
    fn provider_responses_parse_completion_text() {
        let anthropic = json!({
            "content": [
                { "type": "text", "text": "  Anthropic answer.  " }
            ]
        });
        let openai = json!({
            "choices": [
                { "message": { "content": "  OpenAI-compatible answer.  " } }
            ]
        });

        assert_eq!(
            parse_provider_response("anthropic", &anthropic).unwrap(),
            "Anthropic answer."
        );
        assert_eq!(
            parse_provider_response("openai_compatible", &openai).unwrap(),
            "OpenAI-compatible answer."
        );
    }
}
