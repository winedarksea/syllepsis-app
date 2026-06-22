//! Device-local cloud LLM credential commands.
//!
//! API keys must never be written to book config or markdown. The desktop shell stores them in
//! the OS keychain and exposes only boolean status to the UI; the frontend can then execute cloud
//! calls with the Vercel AI SDK and return completions to Rust for proposal wrapping.

use serde::{Deserialize, Serialize};

const KEYCHAIN_SERVICE: &str = "syllepsis.llm";
const API_KEY_FIELD: &str = "api-key";
const BASE_URL_FIELD: &str = "base-url";

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
}
