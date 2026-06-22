//! Device-local cloud LLM credential commands.
//!
//! API keys must never be written to book config or markdown. The desktop shell stores them in
//! the OS keychain and exposes only boolean status to the UI. Provider execution also happens in
//! the shell so secrets do not cross the IPC boundary.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use syllepsis_core::app::llm::{self as app, CloudLlmCompletion, CloudLlmPrompt};
use syllepsis_core::config::ModelRef;
use syllepsis_core::llm::{LlmTask, Proposal};

use crate::state::AppState;

const KEYCHAIN_SERVICE: &str = "syllepsis.llm";
const API_KEY_FIELD: &str = "api-key";
const BASE_URL_FIELD: &str = "base-url";
const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmProviderDescriptor {
    pub provider: String,
    pub display_name: String,
    pub base_url_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudLlmProviderStatus {
    pub provider: String,
    pub display_name: String,
    pub api_key_configured: bool,
    pub base_url_configured: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CloudLlmCredentials {
    api_key: Option<String>,
    base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloudLlmHttpRequest {
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

trait CredentialStore {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&mut self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&mut self, account: &str) -> Result<(), String>;
}

struct KeyringCredentialStore;

impl CredentialStore for KeyringCredentialStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("read keychain entry: {e}")),
        }
    }

    fn set(&mut self, account: &str, secret: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        entry
            .set_password(secret)
            .map_err(|e| format!("write keychain entry: {e}"))
    }

    fn delete(&mut self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("delete keychain entry: {e}")),
        }
    }
}

/// Built-in cloud provider descriptors known to the management UI.
#[tauri::command]
pub fn cloud_llm_provider_descriptors() -> Vec<CloudLlmProviderDescriptor> {
    provider_descriptors()
}

/// Boolean credential status only; never returns stored secrets.
#[tauri::command]
pub fn cloud_llm_provider_statuses() -> Result<Vec<CloudLlmProviderStatus>, String> {
    statuses(&KeyringCredentialStore)
}

/// Save or clear provider credentials in the OS keychain.
#[tauri::command]
pub fn save_cloud_llm_provider_settings(
    settings: CloudLlmProviderSettings,
) -> Result<CloudLlmProviderStatus, String> {
    let mut store = KeyringCredentialStore;
    save_settings(&mut store, settings)
}

/// Clear all credential fields for a provider.
#[tauri::command]
pub fn clear_cloud_llm_provider_settings(
    provider: String,
) -> Result<CloudLlmProviderStatus, String> {
    let mut store = KeyringCredentialStore;
    clear_settings(&mut store, &provider)
}

pub(crate) fn cloud_provider_is_configured(provider: &str) -> Result<bool, String> {
    provider_is_configured(&KeyringCredentialStore, provider)
}

/// Generate a proposal through a configured cloud or OpenAI-compatible local server.
#[tauri::command]
pub fn generate_cloud_proposal(
    state: State<AppState>,
    note_id: String,
    task: LlmTask,
    model_override: Option<ModelRef>,
) -> Result<Proposal, String> {
    let prompt = {
        let guard = state.book.lock().unwrap();
        let book = guard
            .as_ref()
            .ok_or_else(|| "no book is open".to_string())?;
        app::prepare_cloud_prompt(book, &note_id, task, model_override)
            .map_err(|e| e.to_string())?
    };
    let content = execute_cloud_prompt(&KeyringCredentialStore, &prompt)?;
    proposal_from_completed_prompt(&state, prompt, content)
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

fn statuses(store: &impl CredentialStore) -> Result<Vec<CloudLlmProviderStatus>, String> {
    provider_descriptors()
        .into_iter()
        .map(|descriptor| status_for(store, descriptor))
        .collect()
}

fn save_settings(
    store: &mut impl CredentialStore,
    settings: CloudLlmProviderSettings,
) -> Result<CloudLlmProviderStatus, String> {
    let descriptor = descriptor_for(&settings.provider)?;
    apply_optional_secret(
        store,
        &account(&settings.provider, API_KEY_FIELD),
        settings.api_key,
    )?;
    apply_optional_secret(
        store,
        &account(&settings.provider, BASE_URL_FIELD),
        settings.base_url,
    )?;
    status_for(store, descriptor)
}

fn clear_settings(
    store: &mut impl CredentialStore,
    provider: &str,
) -> Result<CloudLlmProviderStatus, String> {
    let descriptor = descriptor_for(provider)?;
    store.delete(&account(provider, API_KEY_FIELD))?;
    store.delete(&account(provider, BASE_URL_FIELD))?;
    status_for(store, descriptor)
}

fn apply_optional_secret(
    store: &mut impl CredentialStore,
    account: &str,
    maybe_secret: Option<String>,
) -> Result<(), String> {
    match maybe_secret {
        None => Ok(()),
        Some(secret) if secret.trim().is_empty() => store.delete(account),
        Some(secret) => store.set(account, secret.trim()),
    }
}

fn status_for(
    store: &impl CredentialStore,
    descriptor: CloudLlmProviderDescriptor,
) -> Result<CloudLlmProviderStatus, String> {
    let api_key_configured = store
        .get(&account(&descriptor.provider, API_KEY_FIELD))?
        .is_some_and(|secret| !secret.trim().is_empty());
    let base_url_configured = store
        .get(&account(&descriptor.provider, BASE_URL_FIELD))?
        .is_some_and(|secret| !secret.trim().is_empty());

    Ok(CloudLlmProviderStatus {
        provider: descriptor.provider,
        display_name: descriptor.display_name,
        api_key_configured,
        base_url_configured,
        base_url_required: descriptor.base_url_required,
    })
}

fn descriptor_for(provider: &str) -> Result<CloudLlmProviderDescriptor, String> {
    provider_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.provider == provider)
        .ok_or_else(|| format!("unknown cloud LLM provider: {provider}"))
}

fn execute_cloud_prompt(
    store: &impl CredentialStore,
    prompt: &CloudLlmPrompt,
) -> Result<String, String> {
    let credentials = credentials_for(store, &prompt.provider)?;
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
    state: &State<AppState>,
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
    store: &impl CredentialStore,
    provider: &str,
) -> Result<CloudLlmCredentials, String> {
    descriptor_for(provider)?;
    Ok(CloudLlmCredentials {
        api_key: trimmed_secret(store.get(&account(provider, API_KEY_FIELD))?),
        base_url: trimmed_secret(store.get(&account(provider, BASE_URL_FIELD))?),
    })
}

fn provider_is_configured(store: &impl CredentialStore, provider: &str) -> Result<bool, String> {
    let credentials = credentials_for(store, provider)?;
    match provider {
        "anthropic" => Ok(credentials.api_key.is_some()),
        "openai_compatible" => Ok(credentials.base_url.is_some()),
        provider => Err(format!("unknown cloud LLM provider: {provider}")),
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

fn account(provider: &str, field: &str) -> String {
    format!("{provider}:{field}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryCredentialStore {
        values: BTreeMap<String, String>,
    }

    impl CredentialStore for MemoryCredentialStore {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.get(account).cloned())
        }

        fn set(&mut self, account: &str, secret: &str) -> Result<(), String> {
            self.values.insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&mut self, account: &str) -> Result<(), String> {
            self.values.remove(account);
            Ok(())
        }
    }

    #[test]
    fn statuses_report_only_presence_not_secret_values() {
        let mut store = MemoryCredentialStore::default();
        store
            .set(&account("anthropic", API_KEY_FIELD), "sk-secret")
            .unwrap();

        let anthropic = statuses(&store)
            .unwrap()
            .into_iter()
            .find(|status| status.provider == "anthropic")
            .unwrap();

        assert!(anthropic.api_key_configured);
        assert!(!anthropic.base_url_configured);
        assert!(!anthropic.base_url_required);
    }

    #[test]
    fn save_settings_trims_and_preserves_unspecified_fields() {
        let mut store = MemoryCredentialStore::default();
        save_settings(
            &mut store,
            CloudLlmProviderSettings {
                provider: "openai_compatible".to_string(),
                api_key: Some("  key  ".to_string()),
                base_url: Some(" http://localhost:8080/v1 ".to_string()),
            },
        )
        .unwrap();

        let status = save_settings(
            &mut store,
            CloudLlmProviderSettings {
                provider: "openai_compatible".to_string(),
                api_key: None,
                base_url: Some(String::new()),
            },
        )
        .unwrap();

        assert!(status.api_key_configured);
        assert!(!status.base_url_configured);
        assert_eq!(
            store
                .get(&account("openai_compatible", API_KEY_FIELD))
                .unwrap(),
            Some("key".to_string())
        );
    }

    #[test]
    fn clear_settings_removes_all_provider_fields() {
        let mut store = MemoryCredentialStore::default();
        store
            .set(&account("anthropic", API_KEY_FIELD), "sk-secret")
            .unwrap();

        let status = clear_settings(&mut store, "anthropic").unwrap();

        assert!(!status.api_key_configured);
        assert!(!status.base_url_configured);
    }

    #[test]
    fn unknown_provider_is_rejected() {
        let mut store = MemoryCredentialStore::default();
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
        let mut store = MemoryCredentialStore::default();
        assert!(!provider_is_configured(&store, "anthropic").unwrap());
        assert!(!provider_is_configured(&store, "openai_compatible").unwrap());

        store
            .set(&account("anthropic", API_KEY_FIELD), "sk-secret")
            .unwrap();
        store
            .set(
                &account("openai_compatible", BASE_URL_FIELD),
                "http://localhost:8080/v1",
            )
            .unwrap();

        assert!(provider_is_configured(&store, "anthropic").unwrap());
        assert!(provider_is_configured(&store, "openai_compatible").unwrap());
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
