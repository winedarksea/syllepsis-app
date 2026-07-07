//! Applying a [`BookKey`] to a note's `summary`/`body` fields.
//!
//! Ciphertext lives in the same frontmatter/body slots plaintext would occupy (base64-encoded),
//! so a locked note round-trips through the existing `serialize_note`/`parse_note` markdown layer
//! untouched. [`encrypt_for_save`] is the sync-safety linchpin: re-encrypting only when the
//! plaintext actually changed keeps unchanged notes byte-identical across saves, which is what
//! lets the CRDT layer's `set_text` no-op and avoid a resync loop.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;

use super::BookKey;
use crate::error::{CoreError, CoreResult};
use crate::model::{EncryptionMeta, Note};

const NONCE_LEN: usize = 24;

fn field_aad(ulid: &str, field: &str) -> Vec<u8> {
    format!("{ulid}:{field}:v1").into_bytes()
}

fn encrypt_field(plain: &str, key: &BookKey, aad: &[u8]) -> CoreResult<(String, String)> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.bytes()));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce_bytes),
            Payload {
                msg: plain.as_bytes(),
                aad,
            },
        )
        .map_err(|_| CoreError::PinLock("note encryption failed".to_string()))?;
    Ok((B64.encode(nonce_bytes), B64.encode(ciphertext)))
}

fn decrypt_field(nonce_b64: &str, ciphertext_b64: &str, key: &BookKey, aad: &[u8]) -> CoreResult<String> {
    let nonce = B64
        .decode(nonce_b64)
        .map_err(|error| CoreError::PinLock(format!("malformed nonce: {error}")))?;
    let ciphertext = B64
        .decode(ciphertext_b64)
        .map_err(|error| CoreError::PinLock(format!("malformed ciphertext: {error}")))?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.bytes()));
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| CoreError::PinLock("note modified externally or wrong key".to_string()))?;
    String::from_utf8(plain)
        .map_err(|_| CoreError::PinLock("decrypted note content was not valid UTF-8".to_string()))
}

/// Encrypt `note`'s current plaintext `summary`/`body` in place under `key`, stamping
/// [`EncryptionMeta`]. Refuses asset/table notes ([`Note::supports_pin_lock`]) and re-locking an
/// already-locked note (use [`decrypt_note`] first if re-encrypting under a new key).
pub fn encrypt_note(note: &mut Note, key: &BookKey) -> CoreResult<()> {
    if !note.supports_pin_lock() {
        return Err(CoreError::PinLock(format!(
            "{} notes cannot be PIN-locked in phase 1",
            note.object_type
        )));
    }
    if note.encryption.is_some() {
        return Err(CoreError::PinLock("note is already locked".to_string()));
    }
    let ulid = note.id.ulid().to_string();
    let (summary_nonce, summary_cipher) =
        encrypt_field(&note.summary, key, &field_aad(&ulid, "summary"))?;
    let (body_nonce, body_cipher) = encrypt_field(&note.body, key, &field_aad(&ulid, "body"))?;
    note.summary = summary_cipher;
    note.body = body_cipher;
    note.encryption = Some(EncryptionMeta::new(
        key.key_id().to_string(),
        summary_nonce,
        body_nonce,
    ));
    Ok(())
}

/// Decrypt `note` and return a copy with `summary`/`body` restored to plaintext and `encryption`
/// cleared. The stored note on disk is untouched — callers decide whether to persist the result
/// (unlock-toggle-off) or just use it in memory (session-unlocked read path).
pub fn decrypt_note(note: &Note, key: &BookKey) -> CoreResult<Note> {
    let meta = note
        .encryption
        .as_ref()
        .ok_or_else(|| CoreError::PinLock("note is not locked".to_string()))?;
    if meta.key_id != key.key_id() {
        return Err(CoreError::PinLock(
            "note was encrypted under a different key".to_string(),
        ));
    }
    let ulid = note.id.ulid().to_string();
    let summary = decrypt_field(&meta.summary_nonce, &note.summary, key, &field_aad(&ulid, "summary"))?;
    let body = decrypt_field(&meta.body_nonce, &note.body, key, &field_aad(&ulid, "body"))?;
    let mut plain = note.clone();
    plain.summary = summary;
    plain.body = body;
    plain.encryption = None;
    Ok(plain)
}

/// Re-encrypt `note`'s summary/body for `new_summary`/`new_body` under `key`, but **only** where
/// the plaintext actually changed: each field is first decrypted and compared against the
/// incoming value, and if they match the stored nonce+ciphertext bytes are left untouched. This is
/// what makes saving a locked note whose content didn't change produce byte-identical output, so
/// the CRDT `set_text` sees a no-op instead of manufacturing a new (ciphertext) edit every save.
///
/// Requires `note.encryption` to already be set (the note must already be locked) and `key` to
/// match its `key_id`.
pub fn encrypt_for_save(note: &mut Note, new_summary: &str, new_body: &str, key: &BookKey) -> CoreResult<()> {
    let meta = note
        .encryption
        .clone()
        .ok_or_else(|| CoreError::PinLock("note is not locked".to_string()))?;
    if meta.key_id != key.key_id() {
        return Err(CoreError::PinLock(
            "note was encrypted under a different key".to_string(),
        ));
    }
    let ulid = note.id.ulid().to_string();

    let summary_aad = field_aad(&ulid, "summary");
    let (summary_nonce, summary_cipher) = match decrypt_field(&meta.summary_nonce, &note.summary, key, &summary_aad) {
        Ok(current) if current == new_summary => (meta.summary_nonce.clone(), note.summary.clone()),
        _ => encrypt_field(new_summary, key, &summary_aad)?,
    };

    let body_aad = field_aad(&ulid, "body");
    let (body_nonce, body_cipher) = match decrypt_field(&meta.body_nonce, &note.body, key, &body_aad) {
        Ok(current) if current == new_body => (meta.body_nonce.clone(), note.body.clone()),
        _ => encrypt_field(new_body, key, &body_aad)?,
    };

    note.summary = summary_cipher;
    note.body = body_cipher;
    note.encryption = Some(EncryptionMeta::new(key.key_id().to_string(), summary_nonce, body_nonce));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    fn key() -> BookKey {
        BookKey::new([7u8; 32], "abcd1234".to_string())
    }

    fn locked_note() -> Note {
        let mut note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        note.summary = "the summary".to_string();
        note.body = "the body text".to_string();
        encrypt_note(&mut note, &key()).unwrap();
        note
    }

    #[test]
    fn round_trips_summary_and_body() {
        let note = locked_note();
        assert!(note.encryption.is_some());
        assert_ne!(note.summary, "the summary");
        assert_ne!(note.body, "the body text");

        let plain = decrypt_note(&note, &key()).unwrap();
        assert_eq!(plain.summary, "the summary");
        assert_eq!(plain.body, "the body text");
        assert!(plain.encryption.is_none());
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let note = locked_note();
        let wrong_key = BookKey::new([9u8; 32], "abcd1234".to_string());
        assert!(decrypt_note(&note, &wrong_key).is_err());
    }

    #[test]
    fn mismatched_key_id_is_rejected_before_touching_ciphertext() {
        let note = locked_note();
        let different_id_key = BookKey::new([7u8; 32], "ffffffff".to_string());
        assert!(decrypt_note(&note, &different_id_key).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails_aead() {
        let mut note = locked_note();
        let mut bytes = B64.decode(&note.body).unwrap();
        bytes[0] ^= 0xFF;
        note.body = B64.encode(bytes);
        assert!(decrypt_note(&note, &key()).is_err());
    }

    #[test]
    fn ciphertext_cannot_be_swapped_between_fields() {
        // AAD binds ciphertext to its field; swapping summary <-> body ciphertext must fail even
        // though both were encrypted under the same key and note ulid.
        let note = locked_note();
        let meta = note.encryption.clone().unwrap();
        let swapped_aad = field_aad(note.id.ulid(), "summary");
        assert!(decrypt_field(&meta.body_nonce, &note.body, &key(), &swapped_aad).is_err());
    }

    #[test]
    fn ciphertext_cannot_be_swapped_between_notes() {
        let note_a = locked_note();
        let mut note_b = Note::new(ObjectType::Note, "Other", "syllepsis_001");
        note_b.summary = "other summary".to_string();
        note_b.body = "other body".to_string();
        encrypt_note(&mut note_b, &key()).unwrap();

        // Graft note A's ciphertext (and its own valid nonce) onto note B's frontmatter.
        let mut grafted = note_b.clone();
        grafted.body = note_a.body.clone();
        grafted.encryption = Some(EncryptionMeta::new(
            key().key_id().to_string(),
            note_b.encryption.as_ref().unwrap().summary_nonce.clone(),
            note_a.encryption.as_ref().unwrap().body_nonce.clone(),
        ));
        assert!(decrypt_note(&grafted, &key()).is_err());
    }

    #[test]
    fn asset_notes_refuse_lock() {
        let mut picture = Note::new(ObjectType::Picture, "Photo", "syllepsis_001");
        picture.summary = "caption".to_string();
        assert!(encrypt_note(&mut picture, &key()).is_err());
    }

    #[test]
    fn encrypt_for_save_is_byte_stable_when_plaintext_unchanged() {
        let mut note = locked_note();
        let before_summary = note.summary.clone();
        let before_body = note.body.clone();
        let before_meta = note.encryption.clone().unwrap();

        encrypt_for_save(&mut note, "the summary", "the body text", &key()).unwrap();

        assert_eq!(note.summary, before_summary);
        assert_eq!(note.body, before_body);
        assert_eq!(note.encryption.unwrap(), before_meta);
    }

    #[test]
    fn encrypt_for_save_re_encrypts_when_plaintext_changes() {
        let mut note = locked_note();
        let before_body = note.body.clone();

        encrypt_for_save(&mut note, "the summary", "changed body text", &key()).unwrap();

        assert_ne!(note.body, before_body);
        let plain = decrypt_note(&note, &key()).unwrap();
        assert_eq!(plain.summary, "the summary");
        assert_eq!(plain.body, "changed body text");
    }

    #[test]
    fn encrypt_for_save_changes_only_the_field_that_changed() {
        let mut note = locked_note();
        let before_summary_nonce = note.encryption.as_ref().unwrap().summary_nonce.clone();

        encrypt_for_save(&mut note, "the summary", "changed body", &key()).unwrap();

        // Summary was untouched, so its nonce (and thus ciphertext) stays identical.
        assert_eq!(
            note.encryption.as_ref().unwrap().summary_nonce,
            before_summary_nonce
        );
    }
}
