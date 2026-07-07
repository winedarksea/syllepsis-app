//! Per-note frontmatter marker for PIN-locked notes (privacy-security.md "PIN-Locked Notes").
//!
//! Presence of [`EncryptionMeta`] on a [`super::Note`] means `summary`/`body` on disk are
//! XChaCha20-Poly1305 ciphertext (base64), not plaintext. The struct itself carries no secret
//! material — only enough to verify which key encrypted the note and to bind ciphertext to a
//! nonce. See [`crate::pinlock`] for the encrypt/decrypt operations.

use serde::{Deserialize, Serialize};

/// Frontmatter-visible record that a note's `summary`/`body` are ciphertext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionMeta {
    pub version: u32,
    /// Always `"xchacha20poly1305"` today; kept as a string (not an enum) so an old app version
    /// that cannot decrypt a future algorithm still round-trips the field instead of failing to
    /// parse the note's frontmatter.
    pub algorithm: String,
    /// The 8-hex `key_id` of the book key this note was encrypted under (see
    /// `pinlock::keycheck::KeyCheck::key_id`). Lets the app detect a stale/rotated key before
    /// attempting to decrypt.
    pub key_id: String,
    /// Base64 24-byte XChaCha20 nonce for the `summary` field.
    pub summary_nonce: String,
    /// Base64 24-byte XChaCha20 nonce for the `body` field.
    pub body_nonce: String,
}

/// Current [`EncryptionMeta::version`] written by this app.
pub const ENCRYPTION_META_VERSION: u32 = 1;
/// Current [`EncryptionMeta::algorithm`] written by this app.
pub const ALGORITHM_XCHACHA20POLY1305: &str = "xchacha20poly1305";

impl EncryptionMeta {
    pub fn new(
        key_id: impl Into<String>,
        summary_nonce: impl Into<String>,
        body_nonce: impl Into<String>,
    ) -> Self {
        EncryptionMeta {
            version: ENCRYPTION_META_VERSION,
            algorithm: ALGORITHM_XCHACHA20POLY1305.to_string(),
            key_id: key_id.into(),
            summary_nonce: summary_nonce.into(),
            body_nonce: body_nonce.into(),
        }
    }
}
