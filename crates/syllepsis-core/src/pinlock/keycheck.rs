//! The synced, content-free keycheck payload (`_pinlock.json`) that lets any device verify a PIN
//! against the book's key without ever storing (or needing) note content.
//!
//! This is the user's "encrypt a known constant" idea done safely: AEAD-decrypting
//! [`KEY_CHECK_CONSTANT`] proves the candidate key is correct without weakening or reusing the key
//! material notes are encrypted under (it's the *same* key, but the check itself leaks nothing
//! beyond "right or wrong").

use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::BookKey;
use crate::error::{CoreError, CoreResult};
use crate::storage::layout;

/// A constant known to every build; encrypting/decrypting it under the candidate key is the PIN
/// check. Never treated as secret.
const KEY_CHECK_CONSTANT: &[u8] = b"syllepsis-pin-check-v1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEYCHECK_VERSION: u32 = 1;

/// Argon2id parameters, stored alongside the salt for future tuning agility (a book created under
/// today's defaults keeps working even if a later version raises them for new books).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    /// OWASP Argon2id baseline (~100-300ms on mid mobile hardware).
    fn default() -> Self {
        KdfParams {
            algorithm: "argon2id".to_string(),
            m_kib: 19456,
            t: 2,
            p: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCheckPayload {
    /// Base64 24-byte XChaCha20 nonce.
    pub nonce: String,
    /// Base64 AEAD ciphertext of [`KEY_CHECK_CONSTANT`].
    pub ciphertext: String,
}

/// `_pinlock.json`: everything another device needs to verify a PIN and derive the book key,
/// containing no note content. Synced as a plain book-root file (never `_sync/`, which is
/// device-local-only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCheck {
    pub version: u32,
    pub kdf: KdfParams,
    /// Base64 16-byte salt.
    pub salt: String,
    pub key_check: KeyCheckPayload,
    /// 8 hex chars of `sha256(salt || key_check.ciphertext)`; stamped onto every locked note's
    /// [`crate::model::EncryptionMeta`] so a stale/rotated key is detected before producing
    /// garbage.
    pub key_id: String,
    /// User-written plaintext hint (the only forgotten-PIN recovery aid: there is none other).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub created_at: DateTime<Utc>,
}

fn derive_raw_key(pin: &str, salt: &[u8], params: &KdfParams) -> CoreResult<[u8; 32]> {
    if params.algorithm != "argon2id" {
        return Err(CoreError::PinLock(format!(
            "unsupported kdf algorithm: {}",
            params.algorithm
        )));
    }
    let argon_params = argon2::Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|error| CoreError::PinLock(format!("invalid argon2 parameters: {error}")))?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, argon_params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(pin.as_bytes(), salt, &mut key)
        .map_err(|error| CoreError::PinLock(format!("key derivation failed: {error}")))?;
    Ok(key)
}

fn compute_key_id(salt: &[u8], ciphertext: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(ciphertext);
    let digest = hasher.finalize();
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_b64(field: &str, value: &str) -> CoreResult<Vec<u8>> {
    B64.decode(value)
        .map_err(|error| CoreError::PinLock(format!("malformed {field}: {error}")))
}

/// Derive the key for `pin` against an already-loaded keycheck's salt/KDF params. Does **not**
/// verify the PIN is correct — [`verify_pin`] is the entry point that actually confirms it via the
/// AEAD key-check payload. Exposed separately so callers that already trust a pin (e.g. re-encrypting
/// under a brand-new keycheck during a PIN change) can derive without re-running verification.
pub fn derive_key(keycheck: &KeyCheck, pin: &str) -> CoreResult<BookKey> {
    let salt = decode_b64("salt", &keycheck.salt)?;
    let raw = derive_raw_key(pin, &salt, &keycheck.kdf)?;
    Ok(BookKey::new(raw, keycheck.key_id.clone()))
}

/// Create a fresh keycheck for `pin`, persist it at `root`/`_pinlock.json`, and return the
/// derived book key. Overwrites any existing keycheck (callers are responsible for the
/// re-encrypt-every-locked-note dance on a PIN change — see `app::pinlock::change_book_pin`).
pub fn create_pinlock(root: &Path, pin: &str, hint: Option<String>) -> CoreResult<BookKey> {
    let mut salt = [0u8; SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    let kdf = KdfParams::default();
    let raw_key = derive_raw_key(pin, &salt, &kdf)?;

    let cipher = XChaCha20Poly1305::new(Key::from_slice(&raw_key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce_bytes), KEY_CHECK_CONSTANT)
        .map_err(|_| CoreError::PinLock("key-check encryption failed".to_string()))?;

    let key_id = compute_key_id(&salt, &ciphertext);
    let keycheck = KeyCheck {
        version: KEYCHECK_VERSION,
        kdf,
        salt: B64.encode(salt),
        key_check: KeyCheckPayload {
            nonce: B64.encode(nonce_bytes),
            ciphertext: B64.encode(&ciphertext),
        },
        key_id: key_id.clone(),
        hint,
        created_at: Utc::now(),
    };

    crate::storage::atomic::write_atomic(
        &layout::pinlock_path(root),
        serde_json::to_string_pretty(&keycheck)?.as_bytes(),
    )?;

    Ok(BookKey::new(raw_key, key_id))
}

/// Load the book's keycheck payload. `NotFound` (via [`CoreError::Io`]) means no PIN is
/// configured yet.
pub fn load_pinlock(root: &Path) -> CoreResult<KeyCheck> {
    let bytes = std::fs::read(layout::pinlock_path(root))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Verify `pin` against the book's keycheck, returning the derived key only if it actually
/// AEAD-decrypts the key-check constant (i.e. is the correct PIN).
pub fn verify_pin(root: &Path, pin: &str) -> CoreResult<BookKey> {
    let keycheck = load_pinlock(root)?;
    let key = derive_key(&keycheck, pin)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.bytes()));
    let nonce = decode_b64("key_check.nonce", &keycheck.key_check.nonce)?;
    let ciphertext = decode_b64("key_check.ciphertext", &keycheck.key_check.ciphertext)?;
    let plain = cipher
        .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| CoreError::PinLock("incorrect PIN".to_string()))?;
    if plain != KEY_CHECK_CONSTANT {
        return Err(CoreError::PinLock("incorrect PIN".to_string()));
    }
    Ok(key)
}

/// Delete the book's keycheck (PIN fully removed). Notes must already have been unlocked by the
/// caller — this only removes the ability to derive/verify a key going forward.
pub fn remove_pinlock(root: &Path) -> CoreResult<()> {
    let path = layout::pinlock_path(root);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Whether the book has a PIN configured.
pub fn is_configured(root: &Path) -> bool {
    layout::pinlock_path(root).is_file()
}

/// Update just the user-written hint, leaving the salt/KDF params/key-check payload (and
/// therefore the derived key and `key_id`) untouched.
pub fn set_hint(root: &Path, hint: Option<String>) -> CoreResult<()> {
    let mut keycheck = load_pinlock(root)?;
    keycheck.hint = hint;
    crate::storage::atomic::write_atomic(
        &layout::pinlock_path(root),
        serde_json::to_string_pretty(&keycheck)?.as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_verify_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let key = create_pinlock(dir.path(), "1234", Some("my hint".to_string())).unwrap();
        assert!(is_configured(dir.path()));

        let verified = verify_pin(dir.path(), "1234").unwrap();
        assert_eq!(verified.key_id(), key.key_id());
        assert_eq!(verified.bytes(), key.bytes());
    }

    #[test]
    fn wrong_pin_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        create_pinlock(dir.path(), "1234", None).unwrap();
        assert!(verify_pin(dir.path(), "0000").is_err());
    }

    #[test]
    fn keycheck_json_round_trips_through_load() {
        let dir = tempfile::tempdir().unwrap();
        create_pinlock(dir.path(), "1234", Some("hint text".to_string())).unwrap();
        let loaded = load_pinlock(dir.path()).unwrap();
        assert_eq!(loaded.kdf, KdfParams::default());
        assert_eq!(loaded.hint.as_deref(), Some("hint text"));
        assert_eq!(loaded.key_id.len(), 8);
    }

    #[test]
    fn set_hint_updates_hint_without_changing_the_key() {
        let dir = tempfile::tempdir().unwrap();
        create_pinlock(dir.path(), "1234", None).unwrap();
        let before = load_pinlock(dir.path()).unwrap();

        set_hint(dir.path(), Some("new hint".to_string())).unwrap();

        let after = load_pinlock(dir.path()).unwrap();
        assert_eq!(after.hint.as_deref(), Some("new hint"));
        assert_eq!(after.key_id, before.key_id);
        assert_eq!(after.salt, before.salt);
        // The derived key is unaffected — the same PIN still verifies.
        assert!(verify_pin(dir.path(), "1234").is_ok());
    }

    #[test]
    fn remove_pinlock_deletes_file() {
        let dir = tempfile::tempdir().unwrap();
        create_pinlock(dir.path(), "1234", None).unwrap();
        remove_pinlock(dir.path()).unwrap();
        assert!(!is_configured(dir.path()));
        assert!(load_pinlock(dir.path()).is_err());
    }

    #[test]
    fn different_pins_derive_different_keys() {
        let dir = tempfile::tempdir().unwrap();
        let a = create_pinlock(dir.path(), "1234", None).unwrap();
        remove_pinlock(dir.path()).unwrap();
        let b = create_pinlock(dir.path(), "5678", None).unwrap();
        assert_ne!(a.bytes(), b.bytes());
    }
}
