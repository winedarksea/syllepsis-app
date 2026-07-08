//! PIN-locked notes (privacy-security.md "PIN-Locked Notes").
//!
//! One PIN protects an entire book. [`keycheck`] derives and verifies the book key against a
//! synced-but-content-free `_pinlock.json` payload; [`notecrypt`] applies that key to individual
//! notes' `summary`/`body` fields. Sync never needs the key: ciphertext is the on-disk source of
//! truth, and [`notecrypt::encrypt_for_save`] is what keeps unchanged plaintext byte-stable across
//! saves so the CRDT layer sees a no-op (no sync loops).
//!
//! Threat model is *moderate* (resist casual snooping / file browsing, not a nation-state
//! adversary): XChaCha20-Poly1305 AEAD with an Argon2id-derived key, a random 24-byte nonce per
//! field, and an AAD binding ciphertext to `{note ulid}:{field}:v1` so ciphertext cannot be
//! swapped between notes or fields without failing authentication.

pub mod keycheck;
pub mod notecrypt;

pub use keycheck::{
    create_pinlock, load_pinlock, remove_pinlock, verify_pin, KdfParams, KeyCheck, KeyCheckPayload,
};
pub use notecrypt::{decrypt_note, decrypt_note_with_recovered_body, encrypt_for_save, encrypt_note};

use zeroize::Zeroizing;

/// The book's derived symmetric key, zeroized on drop. Carries the `key_id` it was derived
/// under (the keycheck's fingerprint) so callers can stamp/verify [`crate::model::EncryptionMeta`]
/// without re-deriving or re-reading the keycheck file.
#[derive(Clone)]
pub struct BookKey {
    key: Zeroizing<[u8; 32]>,
    key_id: String,
}

impl BookKey {
    pub fn new(key: [u8; 32], key_id: impl Into<String>) -> Self {
        BookKey {
            key: Zeroizing::new(key),
            key_id: key_id.into(),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub(crate) fn bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Expose the raw key bytes. The one legitimate reason to let this leave process memory: the
    /// tauri boundary's "remember on this device" vault write (and the matching read to
    /// reconstruct a [`BookKey`] via [`BookKey::new`]). Not used anywhere in `syllepsis-core`
    /// itself — the crate never persists a key.
    pub fn expose_for_vault_storage(&self) -> [u8; 32] {
        *self.key
    }
}

impl std::fmt::Debug for BookKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BookKey")
            .field("key_id", &self.key_id)
            .field("key", &"<redacted>")
            .finish()
    }
}
