//! Single keychain "secrets vault" shared by the sync and cloud-LLM subsystems.
//!
//! macOS attaches one access-control list (and therefore one "Always Allow") per keychain *item*,
//! so every distinct item the app touches is a separate prompt. To collapse all persistent secrets
//! behind a single "Always Allow", every secret lives as one JSON document in one keychain item
//! (service [`SECRETS_KEYCHAIN_SERVICE`], account [`VAULT_ACCOUNT`]). On the first launch after this
//! lands we migrate any legacy per-field items written by earlier builds, then delete them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const SECRETS_KEYCHAIN_SERVICE: &str = "syllepsis.secrets";
const DEVELOPMENT_SECRETS_KEYCHAIN_SERVICE: &str = "syllepsis.secrets.dev";
const VAULT_ACCOUNT: &str = "vault";

/// Legacy per-field keychain items, read once during migration and then deleted.
const LEGACY_SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync";
const LEGACY_DEVELOPMENT_SYNC_KEYCHAIN_SERVICE: &str = "syllepsis.sync.dev";
const LEGACY_LLM_KEYCHAIN_SERVICE: &str = "syllepsis.llm";
const ACCESS_TOKEN_FIELD: &str = "access-token";
const REFRESH_TOKEN_FIELD: &str = "refresh-token";
const OAUTH_STATE_FIELD: &str = "oauth-state";
const CODE_VERIFIER_FIELD: &str = "code-verifier";
const API_KEY_FIELD: &str = "api-key";
const BASE_URL_FIELD: &str = "base-url";

/// Sync providers whose legacy items we look for during migration.
const LEGACY_SYNC_PROVIDERS: &[&str] = &["google_drive", "dropbox", "onedrive"];
/// Cloud-LLM providers whose legacy items we look for during migration.
const LEGACY_LLM_PROVIDERS: &[&str] = &["anthropic", "openai_compatible"];

/// OAuth tokens for one managed-sync provider.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTokens {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

/// API credentials for one cloud-LLM provider.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmSecret {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// The whole secret document stored in the single keychain item.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretsVault {
    #[serde(default)]
    pub sync: BTreeMap<String, SyncTokens>,
    #[serde(default)]
    pub llm: BTreeMap<String, LlmSecret>,
}

/// Read/write access to the single vault item plus the legacy items consulted during migration.
pub trait VaultStore {
    fn get_vault(&self) -> Result<Option<String>, String>;
    fn set_vault(&mut self, value: &str) -> Result<(), String>;
    /// Read a legacy per-field item from one of the old services. Returns `None` for missing items.
    fn get_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    /// Delete a legacy per-field item; a missing item is treated as success.
    fn delete_legacy(&mut self, service: &str, account: &str) -> Result<(), String>;
}

/// Vault backed by the OS keychain.
pub struct KeyringVaultStore;

impl KeyringVaultStore {
    pub fn new() -> Self {
        KeyringVaultStore
    }
}

impl Default for KeyringVaultStore {
    fn default() -> Self {
        Self::new()
    }
}

impl VaultStore for KeyringVaultStore {
    fn get_vault(&self) -> Result<Option<String>, String> {
        keyring_get(secrets_keychain_service(), VAULT_ACCOUNT)
    }

    fn set_vault(&mut self, value: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(secrets_keychain_service(), VAULT_ACCOUNT)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        entry
            .set_password(value)
            .map_err(|e| format!("write keychain entry: {e}"))
    }

    fn get_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        keyring_get(service, account)
    }

    fn delete_legacy(&mut self, service: &str, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| format!("open keychain entry: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("delete keychain entry: {e}")),
        }
    }
}

fn keyring_get(service: &str, account: &str) -> Result<Option<String>, String> {
    let entry =
        keyring::Entry::new(service, account).map_err(|e| format!("open keychain entry: {e}"))?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("read keychain entry: {e}")),
    }
}

fn secrets_keychain_service() -> &'static str {
    if cfg!(debug_assertions) {
        DEVELOPMENT_SECRETS_KEYCHAIN_SERVICE
    } else {
        SECRETS_KEYCHAIN_SERVICE
    }
}

fn legacy_sync_keychain_service() -> &'static str {
    if cfg!(debug_assertions) {
        LEGACY_DEVELOPMENT_SYNC_KEYCHAIN_SERVICE
    } else {
        LEGACY_SYNC_KEYCHAIN_SERVICE
    }
}

fn account(provider: &str, field: &str) -> String {
    format!("{provider}:{field}")
}

fn trimmed(secret: Option<String>) -> Option<String> {
    secret
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Load the vault, migrating any legacy per-field items on the first post-update launch.
///
/// Reads exactly one item (the vault) on the hot path. When the vault item is absent we sweep the
/// legacy items: if any secret is found we build the vault, persist it, and delete the legacy items
/// so the migration runs only once. Sweeping missing legacy items is free — a non-existent keychain
/// item never prompts.
pub fn load_vault(store: &mut impl VaultStore) -> Result<SecretsVault, String> {
    if let Some(raw) = store.get_vault()? {
        return parse_vault(&raw);
    }
    let (vault, migrated_any) = migrate_legacy(store)?;
    if migrated_any {
        save_vault(store, &vault)?;
        delete_all_legacy(store)?;
    }
    Ok(vault)
}

fn parse_vault(raw: &str) -> Result<SecretsVault, String> {
    serde_json::from_str(raw).map_err(|e| format!("parse secrets vault: {e}"))
}

/// Persist the vault as the single keychain item.
pub fn save_vault(store: &mut impl VaultStore, vault: &SecretsVault) -> Result<(), String> {
    let raw = serde_json::to_string(vault).map_err(|e| format!("serialize secrets vault: {e}"))?;
    store.set_vault(&raw)
}

fn migrate_legacy(store: &mut impl VaultStore) -> Result<(SecretsVault, bool), String> {
    let mut vault = SecretsVault::default();
    let mut migrated_any = false;

    let sync_service = legacy_sync_keychain_service();
    for &provider in LEGACY_SYNC_PROVIDERS {
        let access = trimmed(store.get_legacy(sync_service, &account(provider, ACCESS_TOKEN_FIELD))?);
        let refresh =
            trimmed(store.get_legacy(sync_service, &account(provider, REFRESH_TOKEN_FIELD))?);
        if access.is_some() || refresh.is_some() {
            vault.sync.insert(
                provider.to_string(),
                SyncTokens {
                    access_token: access,
                    refresh_token: refresh,
                },
            );
            migrated_any = true;
        }
    }

    for &provider in LEGACY_LLM_PROVIDERS {
        let api_key =
            trimmed(store.get_legacy(LEGACY_LLM_KEYCHAIN_SERVICE, &account(provider, API_KEY_FIELD))?);
        let base_url = trimmed(
            store.get_legacy(LEGACY_LLM_KEYCHAIN_SERVICE, &account(provider, BASE_URL_FIELD))?,
        );
        if api_key.is_some() || base_url.is_some() {
            vault
                .llm
                .insert(provider.to_string(), LlmSecret { api_key, base_url });
            migrated_any = true;
        }
    }

    Ok((vault, migrated_any))
}

fn delete_all_legacy(store: &mut impl VaultStore) -> Result<(), String> {
    let sync_service = legacy_sync_keychain_service();
    for &provider in LEGACY_SYNC_PROVIDERS {
        for field in [
            ACCESS_TOKEN_FIELD,
            REFRESH_TOKEN_FIELD,
            OAUTH_STATE_FIELD,
            CODE_VERIFIER_FIELD,
        ] {
            store.delete_legacy(sync_service, &account(provider, field))?;
        }
    }
    for &provider in LEGACY_LLM_PROVIDERS {
        for field in [API_KEY_FIELD, BASE_URL_FIELD] {
            store.delete_legacy(LEGACY_LLM_KEYCHAIN_SERVICE, &account(provider, field))?;
        }
    }
    Ok(())
}

/// Read one provider's sync tokens. `None` means no tokens are stored for that provider.
pub fn read_sync_tokens(
    store: &mut impl VaultStore,
    provider: &str,
) -> Result<Option<SyncTokens>, String> {
    Ok(load_vault(store)?.sync.get(provider).cloned())
}

/// Replace one provider's sync tokens.
pub fn write_sync_tokens(
    store: &mut impl VaultStore,
    provider: &str,
    tokens: SyncTokens,
) -> Result<(), String> {
    let mut vault = load_vault(store)?;
    vault.sync.insert(provider.to_string(), tokens);
    save_vault(store, &vault)
}

/// Remove one provider's sync tokens (and sweep any lingering legacy sync items for it).
pub fn delete_sync_tokens(store: &mut impl VaultStore, provider: &str) -> Result<(), String> {
    let mut vault = load_vault(store)?;
    let removed = vault.sync.remove(provider).is_some();
    if removed {
        save_vault(store, &vault)?;
    }
    let sync_service = legacy_sync_keychain_service();
    for field in [
        ACCESS_TOKEN_FIELD,
        REFRESH_TOKEN_FIELD,
        OAUTH_STATE_FIELD,
        CODE_VERIFIER_FIELD,
    ] {
        store.delete_legacy(sync_service, &account(provider, field))?;
    }
    Ok(())
}

/// Read one provider's LLM secret. `None` means nothing is stored for that provider.
pub fn read_llm_secret(
    store: &mut impl VaultStore,
    provider: &str,
) -> Result<Option<LlmSecret>, String> {
    Ok(load_vault(store)?.llm.get(provider).cloned())
}

/// Replace one provider's LLM secret.
pub fn write_llm_secret(
    store: &mut impl VaultStore,
    provider: &str,
    secret: LlmSecret,
) -> Result<(), String> {
    let mut vault = load_vault(store)?;
    vault.llm.insert(provider.to_string(), secret);
    save_vault(store, &vault)
}

/// Remove one provider's LLM secret.
pub fn delete_llm_secret(store: &mut impl VaultStore, provider: &str) -> Result<(), String> {
    let mut vault = load_vault(store)?;
    if vault.llm.remove(provider).is_some() {
        save_vault(store, &vault)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    /// In-memory [`VaultStore`] for tests, counting vault reads so callers can assert the hot path
    /// touches the keychain exactly once.
    #[derive(Default)]
    pub struct MemoryVaultStore {
        vault: Option<String>,
        legacy: BTreeMap<(String, String), String>,
        vault_get_count: Cell<usize>,
    }

    impl MemoryVaultStore {
        pub fn vault_get_count(&self) -> usize {
            self.vault_get_count.get()
        }

        /// Seed a legacy per-field item as an older build would have written it.
        pub fn seed_legacy(&mut self, service: &str, account: &str, value: &str) {
            self.legacy
                .insert((service.to_string(), account.to_string()), value.to_string());
        }

        pub fn legacy_len(&self) -> usize {
            self.legacy.len()
        }
    }

    impl VaultStore for MemoryVaultStore {
        fn get_vault(&self) -> Result<Option<String>, String> {
            self.vault_get_count.set(self.vault_get_count.get() + 1);
            Ok(self.vault.clone())
        }

        fn set_vault(&mut self, value: &str) -> Result<(), String> {
            self.vault = Some(value.to_string());
            Ok(())
        }

        fn get_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self
                .legacy
                .get(&(service.to_string(), account.to_string()))
                .cloned())
        }

        fn delete_legacy(&mut self, service: &str, account: &str) -> Result<(), String> {
            self.legacy.remove(&(service.to_string(), account.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MemoryVaultStore;
    use super::*;

    #[test]
    fn vault_round_trips_sync_and_llm_secrets() {
        let mut store = MemoryVaultStore::default();
        write_sync_tokens(
            &mut store,
            "dropbox",
            SyncTokens {
                access_token: Some("access".to_string()),
                refresh_token: Some("refresh".to_string()),
            },
        )
        .unwrap();
        write_llm_secret(
            &mut store,
            "anthropic",
            LlmSecret {
                api_key: Some("sk-ant".to_string()),
                base_url: None,
            },
        )
        .unwrap();

        let tokens = read_sync_tokens(&mut store, "dropbox").unwrap().unwrap();
        assert_eq!(tokens.access_token.as_deref(), Some("access"));
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh"));
        let secret = read_llm_secret(&mut store, "anthropic").unwrap().unwrap();
        assert_eq!(secret.api_key.as_deref(), Some("sk-ant"));
        assert!(read_llm_secret(&mut store, "openai_compatible")
            .unwrap()
            .is_none());
    }

    #[test]
    fn cold_read_touches_vault_item_exactly_once() {
        let mut store = MemoryVaultStore::default();
        write_sync_tokens(
            &mut store,
            "dropbox",
            SyncTokens {
                access_token: Some("access".to_string()),
                refresh_token: None,
            },
        )
        .unwrap();
        let before = store.vault_get_count();

        read_sync_tokens(&mut store, "dropbox").unwrap();

        assert_eq!(store.vault_get_count() - before, 1);
    }

    #[test]
    fn migration_builds_vault_from_legacy_items_and_deletes_them() {
        let mut store = MemoryVaultStore::default();
        let sync_service = legacy_sync_keychain_service();
        store.seed_legacy(sync_service, &account("dropbox", ACCESS_TOKEN_FIELD), "access");
        store.seed_legacy(
            sync_service,
            &account("dropbox", REFRESH_TOKEN_FIELD),
            "refresh",
        );
        store.seed_legacy(
            LEGACY_LLM_KEYCHAIN_SERVICE,
            &account("openai_compatible", BASE_URL_FIELD),
            "https://example.test/v1",
        );

        let vault = load_vault(&mut store).unwrap();

        assert_eq!(
            vault.sync.get("dropbox").unwrap().access_token.as_deref(),
            Some("access")
        );
        assert_eq!(
            vault.sync.get("dropbox").unwrap().refresh_token.as_deref(),
            Some("refresh")
        );
        assert_eq!(
            vault.llm.get("openai_compatible").unwrap().base_url.as_deref(),
            Some("https://example.test/v1")
        );
        // Legacy items are removed once migrated, and the migration does not run again.
        assert_eq!(store.legacy_len(), 0);
        assert!(store.get_vault().unwrap().is_some());
    }

    #[test]
    fn absent_vault_with_no_legacy_items_yields_empty_vault_without_writing() {
        let mut store = MemoryVaultStore::default();

        let vault = load_vault(&mut store).unwrap();

        assert_eq!(vault, SecretsVault::default());
        // No secret was found, so no item is created — nothing to prompt for.
        assert!(store.get_vault().unwrap().is_none());
    }

    #[test]
    fn delete_sync_tokens_removes_provider_entry() {
        let mut store = MemoryVaultStore::default();
        write_sync_tokens(
            &mut store,
            "dropbox",
            SyncTokens {
                access_token: Some("access".to_string()),
                refresh_token: None,
            },
        )
        .unwrap();

        delete_sync_tokens(&mut store, "dropbox").unwrap();

        assert!(read_sync_tokens(&mut store, "dropbox").unwrap().is_none());
    }
}
