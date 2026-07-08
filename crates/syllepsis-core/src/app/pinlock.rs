//! Application command surface for PIN-locked notes (privacy-security.md "PIN-Locked Notes").
//!
//! Like the rest of [`crate::app`], these are framework-agnostic functions over a [`Book`] the
//! Tauri shell wraps as commands (`commands::pinlock` there owns the *session* — which key, if
//! any, is currently held in memory — since that's process/UI state, not book state). Everything
//! here either manages the book-wide keycheck (`_pinlock.json`) or performs the lock/unlock
//! transition hygiene a single note's toggle requires: re-encrypt, purge derived plaintext
//! artifacts (embeddings, CRDT history), and let the existing `is_searchable`/`is_publishable`
//! predicates (already keyed on `Note::is_pin_locked`) take care of the rest.

use crate::app::dto::NoteDto;
use crate::crdt::{select_crdt_backend, CrdtBackend, LwwBackend};
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::model::Note;
use crate::pinlock::{self, BookKey};
use crate::storage::atomic::write_atomic;
use crate::storage::{layout, Book, NoteStore};

/// Whether the book has a PIN configured (`_pinlock.json` present).
pub fn is_pin_configured(book: &Book) -> bool {
    pinlock::keycheck::is_configured(&book.root)
}

/// The book's keycheck hint (if a PIN is configured), for the "forgot your PIN?" prompt.
pub fn pin_hint(book: &Book) -> CoreResult<Option<String>> {
    match pinlock::load_pinlock(&book.root) {
        Ok(keycheck) => Ok(keycheck.hint),
        Err(CoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Set the book's PIN for the first time. Fails if one is already configured (use
/// [`change_book_pin`] to rotate it).
pub fn set_book_pin(book: &Book, pin: &str, hint: Option<String>) -> CoreResult<BookKey> {
    if is_pin_configured(book) {
        return Err(CoreError::PinLock(
            "book already has a PIN; use change_book_pin to change it".to_string(),
        ));
    }
    pinlock::create_pinlock(&book.root, pin, hint)
}

/// Verify `pin` against the book's keycheck, returning the derived key on success.
pub fn unlock_book(book: &Book, pin: &str) -> CoreResult<BookKey> {
    pinlock::verify_pin(&book.root, pin)
}

/// Update the book's PIN hint without changing the PIN itself.
pub fn set_pin_hint(book: &Book, hint: Option<String>) -> CoreResult<()> {
    pinlock::keycheck::set_hint(&book.root, hint)
}

/// Change the book's PIN: verify the old one, mint a fresh keycheck (new salt/key), and
/// re-encrypt every currently-locked note under the new key so nothing is left under the old one.
/// The caller (tauri boundary) is responsible for dropping any remembered/vaulted key for this
/// book afterward — the old key is no longer valid.
pub fn change_book_pin(
    book: &Book,
    old_pin: &str,
    new_pin: &str,
    hint: Option<String>,
) -> CoreResult<BookKey> {
    let old_key = pinlock::verify_pin(&book.root, old_pin)?;
    pinlock::remove_pinlock(&book.root)?;
    let new_key = match pinlock::create_pinlock(&book.root, new_pin, hint) {
        Ok(key) => key,
        Err(error) => {
            // Best-effort: don't strand the book with no PIN at all if minting the new keycheck
            // failed partway through (e.g. disk full after the remove).
            return Err(error);
        }
    };

    for mut note in book.store.read_all_notes()? {
        if !note.is_pin_locked() {
            continue;
        }
        let plain = decrypt_note_with_recovery(book, &note, &old_key).map_err(|error| {
            CoreError::PinLock(format!(
                "note {} (\"{}\") could not be decrypted while changing the PIN: {error}",
                note.id, note.title
            ))
        })?;
        note.summary = plain.summary;
        note.body = plain.body;
        note.encryption = None;
        pinlock::encrypt_note(&mut note, &new_key)?;
        book.save_note(&note)?;
    }
    Ok(new_key)
}

/// Remove the book's PIN entirely: every currently-locked note is decrypted back to plaintext
/// first (full unlock hygiene, same as toggling each one off individually), then the keycheck
/// file is deleted. The caller must already hold a verified key (from [`unlock_book`]).
///
/// Stops at the first note that fails to decrypt (rather than silently leaving the book half
/// unlocked) and names it in the error, so a single corrupted note is something the user can go
/// find and deal with instead of a bare "malformed ciphertext" with no note attached.
pub fn remove_book_pin(book: &Book, key: &BookKey) -> CoreResult<()> {
    for note in book.store.read_all_notes()? {
        if note.is_pin_locked() {
            set_note_pin_locked(book, note.id.as_str(), false, key).map_err(|error| {
                CoreError::PinLock(format!(
                    "note {} (\"{}\") could not be unlocked: {error}",
                    note.id, note.title
                ))
            })?;
        }
    }
    pinlock::remove_pinlock(&book.root)
}

/// Lock or unlock a single note. Requires a verified [`BookKey`] either way (unlocking obviously
/// needs it to decrypt; locking needs it because the book must already have a configured PIN to
/// derive `key_id` from).
///
/// **Lock** transition hygiene (privacy-security.md): encrypt summary+body, purge the note's
/// embedding sidecar (so it can never surface via a stale vector), and destroy the plaintext CRDT
/// history by reseeding a fresh, always-available LWW document from the ciphertext body — a
/// pre-existing Loro history could otherwise retain plaintext operations even after the visible
/// text changes. `is_searchable`/`is_publishable` already key off `Note::is_pin_locked`, so search/
/// RAG/publish exclusion falls out automatically; re-enqueuing embeddings and refreshing the search
/// index are the tauri boundary's job (they need the async model-inference worker).
///
/// **Unlock** (toggle off): decrypt, strip `encryption`, save plaintext, and reseed the sidecar
/// with the book's normally-configured backend (Loro or LWW) instead of the locked-note override.
pub fn set_note_pin_locked(
    book: &Book,
    id: &str,
    locked: bool,
    key: &BookKey,
) -> CoreResult<NoteDto> {
    let note_id = NoteId::parse(id)?;
    let mut note = book.store.read_note(&note_id)?;

    if locked {
        if note.is_pin_locked() {
            return Err(CoreError::PinLock("note is already locked".to_string()));
        }
        if !note.supports_pin_lock() {
            return Err(CoreError::PinLock(format!(
                "{} notes cannot be PIN-locked in phase 1",
                note.object_type
            )));
        }
        pinlock::encrypt_note(&mut note, key)?;
        book.save_note(&note)?;
        purge_embedding_sidecar(book, &note.id)?;
        reseed_sidecar(book, &note, &LwwBackend)?;
    } else {
        if !note.is_pin_locked() {
            return Err(CoreError::PinLock("note is not locked".to_string()));
        }
        note = decrypt_note_with_recovery(book, &note, key)?;
        book.save_note(&note)?;
        let backend = select_crdt_backend(&book.config.sync);
        reseed_sidecar(book, &note, backend.as_ref())?;
    }
    Ok(NoteDto::from_note(&note))
}

/// Strict decrypt first; if an older broken lock transition already corrupted the body ciphertext,
/// recover from a plaintext CRDT sidecar when one still exists. This is intentionally narrow:
/// the PIN/key must still decrypt the note summary, and the fallback is used only after AEAD/base64
/// body decrypt fails. On unlock, callers immediately save plaintext and reseed the sidecar.
pub fn decrypt_note_with_recovery(book: &Book, note: &Note, key: &BookKey) -> CoreResult<Note> {
    match pinlock::decrypt_note(note, key) {
        Ok(plain) => Ok(plain),
        Err(decrypt_error) => {
            let recovered_body = recover_body_from_plaintext_artifacts(book, note).map_err(|recovery_error| {
                CoreError::PinLock(format!(
                    "{decrypt_error}; plaintext recovery failed: {recovery_error}"
                ))
            })?;
            pinlock::decrypt_note_with_recovered_body(note, key, recovered_body)
        }
    }
}

fn purge_embedding_sidecar(book: &Book, id: &NoteId) -> CoreResult<()> {
    let path = layout::embedding_sidecar_path(&book.root, id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Overwrite the note's CRDT sidecar with a brand-new document seeded from its current (on-disk)
/// body under `backend`. A full replace — not a merge — so no prior history survives: this is
/// both the "destroy plaintext CRDT history" step on lock and the "restore configured backend"
/// step on unlock.
fn reseed_sidecar(book: &Book, note: &Note, backend: &dyn crate::crdt::CrdtBackend) -> CoreResult<()> {
    let actor = crate::sync::actor_id_for(&book.root)?;
    let doc = backend.new_document(&actor, &note.body);
    write_atomic(&layout::crdt_sidecar_path(&book.root, &note.id), &doc.snapshot()?)
}

fn recover_body_from_plaintext_artifacts(book: &Book, note: &Note) -> CoreResult<String> {
    if let Ok(body) = recover_body_from_crdt_sidecar(book, note) {
        return Ok(body);
    }
    recover_body_from_commentary_base_body(book, note)
}

fn recover_body_from_crdt_sidecar(book: &Book, note: &Note) -> CoreResult<String> {
    let path = layout::crdt_sidecar_path(&book.root, &note.id);
    let bytes = std::fs::read(&path)?;
    let actor = crate::sync::actor_id_for(&book.root)?;
    let configured = select_crdt_backend(&book.config.sync);
    if let Ok(doc) = configured.load_document(&actor, &bytes) {
        let text = doc.text();
        if usable_recovered_body(&text, note) {
            return Ok(text);
        }
    }
    let lww = LwwBackend;
    if configured.name() != lww.name() {
        if let Ok(doc) = lww.load_document(&actor, &bytes) {
            let text = doc.text();
            if usable_recovered_body(&text, note) {
                return Ok(text);
            }
        }
    }
    Err(CoreError::PinLock(
        "no usable plaintext body was found in the CRDT sidecar".to_string(),
    ))
}

fn recover_body_from_commentary_base_body(book: &Book, note: &Note) -> CoreResult<String> {
    for commentary in book.read_all_commentary_notes()? {
        let Some(metadata) = commentary.commentary.as_ref() else {
            continue;
        };
        if metadata.parent_note_id != note.id {
            continue;
        }
        let Some(base_body) = metadata.base_body.as_ref() else {
            continue;
        };
        if usable_recovered_body(base_body, note) {
            return Ok(base_body.clone());
        }
    }
    Err(CoreError::PinLock(
        "no usable plaintext body was found in commentary base bodies".to_string(),
    ))
}

fn usable_recovered_body(text: &str, note: &Note) -> bool {
    !text.is_empty() && text != note.body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::CrdtBackend;
    use crate::model::ObjectType;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    #[test]
    fn set_book_pin_then_unlock_round_trips() {
        let (_d, book) = book();
        assert!(!is_pin_configured(&book));
        let key = set_book_pin(&book, "1234", Some("hint".to_string())).unwrap();
        assert!(is_pin_configured(&book));
        assert_eq!(pin_hint(&book).unwrap().as_deref(), Some("hint"));

        let unlocked = unlock_book(&book, "1234").unwrap();
        assert_eq!(unlocked.key_id(), key.key_id());
        assert!(unlock_book(&book, "0000").is_err());
    }

    #[test]
    fn set_book_pin_refuses_when_already_configured() {
        let (_d, book) = book();
        set_book_pin(&book, "1234", None).unwrap();
        assert!(set_book_pin(&book, "5678", None).is_err());
    }

    #[test]
    fn lock_then_unlock_note_round_trips_content_and_hygiene() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut note = book.new_note(ObjectType::Note, "diary").unwrap();
        note.summary = "q".into();
        note.body = "secret content".into();
        book.save_note(&note).unwrap();
        crate::embeddings::repository::write_test_sidecars(&book, &[note.clone()]);
        assert!(layout::embedding_sidecar_path(&book.root, &note.id).exists());

        let locked_dto = set_note_pin_locked(&book, note.id.as_str(), true, &key).unwrap();
        assert!(locked_dto.pin_locked);
        assert!(!locked_dto.unlocked);
        assert!(locked_dto.summary.is_empty() && locked_dto.body.is_empty());

        let stored = book.store.read_note(&note.id).unwrap();
        assert!(stored.is_pin_locked());
        assert_ne!(stored.body, "secret content");
        // Hygiene: embedding sidecar purged, CRDT sidecar reseeded from ciphertext.
        assert!(!layout::embedding_sidecar_path(&book.root, &note.id).exists());
        let actor = crate::sync::actor_id_for(&book.root).unwrap();
        let sidecar_bytes =
            std::fs::read(layout::crdt_sidecar_path(&book.root, &note.id)).unwrap();
        let doc = LwwBackend.load_document(&actor, &sidecar_bytes).unwrap();
        assert_eq!(doc.text(), stored.body);
        assert!(!stored.is_searchable());

        // Re-locking must fail; asset notes must refuse entirely.
        assert!(set_note_pin_locked(&book, note.id.as_str(), true, &key).is_err());

        let unlocked_dto = set_note_pin_locked(&book, note.id.as_str(), false, &key).unwrap();
        assert!(!unlocked_dto.pin_locked);
        assert_eq!(unlocked_dto.body, "secret content");
        let stored = book.store.read_note(&note.id).unwrap();
        assert!(!stored.is_pin_locked());
        assert_eq!(stored.body, "secret content");
        assert!(stored.is_searchable());
    }

    #[test]
    fn unlock_recovers_from_plaintext_sidecar_when_existing_ciphertext_is_corrupted() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut note = book.new_note(ObjectType::Note, "old broken lock").unwrap();
        note.summary = "short summary".into();
        note.body = "plaintext body left in an old sidecar".into();
        book.save_note(&note).unwrap();

        // Simulate the initial broken implementation: the old plaintext CRDT history was left
        // behind, then the markdown body ciphertext was later corrupted by a text merge.
        let actor = crate::sync::actor_id_for(&book.root).unwrap();
        let backend = select_crdt_backend(&book.config.sync);
        let plaintext_doc = backend.new_document(&actor, &note.body);
        write_atomic(
            &layout::crdt_sidecar_path(&book.root, &note.id),
            &plaintext_doc.snapshot().unwrap(),
        )
        .unwrap();
        pinlock::encrypt_note(&mut note, &key).unwrap();
        note.body.push('A');
        book.save_note(&note).unwrap();
        assert!(pinlock::decrypt_note(&note, &key).is_err());

        let unlocked = set_note_pin_locked(&book, note.id.as_str(), false, &key).unwrap();

        assert!(!unlocked.pin_locked);
        assert_eq!(unlocked.summary, "short summary");
        assert_eq!(unlocked.body, "plaintext body left in an old sidecar");
        let stored = book.store.read_note(&note.id).unwrap();
        assert!(!stored.is_pin_locked());
        assert_eq!(stored.body, "plaintext body left in an old sidecar");
    }

    #[test]
    fn unlock_recovers_from_commentary_base_body_when_sidecar_text_is_corrupted_too() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut note = book.new_note(ObjectType::Note, "old broken lock with commentary").unwrap();
        note.summary = "short summary".into();
        note.body = "body preserved in commentary metadata".into();
        book.save_note(&note).unwrap();

        let mut commentary = book
            .new_commentary_note(
                "rewrite proposal for old broken lock",
                crate::model::CommentaryMetadata::new(
                    note.id.clone(),
                    crate::model::CommentaryKind::Footnote,
                    crate::model::CommentarySource::Ai,
                ),
            )
            .unwrap();
        commentary
            .commentary
            .as_mut()
            .unwrap()
            .base_body = Some(note.body.clone());
        book.save_commentary_note(&commentary).unwrap();

        pinlock::encrypt_note(&mut note, &key).unwrap();
        note.body.push('A');
        book.save_note(&note).unwrap();

        // Simulate the Loro-history failure mode: the sidecar loads, but its current text is the
        // same corrupted ciphertext body as the markdown, so it is not a usable plaintext source.
        let actor = crate::sync::actor_id_for(&book.root).unwrap();
        let backend = select_crdt_backend(&book.config.sync);
        let corrupted_doc = backend.new_document(&actor, &note.body);
        write_atomic(
            &layout::crdt_sidecar_path(&book.root, &note.id),
            &corrupted_doc.snapshot().unwrap(),
        )
        .unwrap();

        let unlocked = set_note_pin_locked(&book, note.id.as_str(), false, &key).unwrap();

        assert!(!unlocked.pin_locked);
        assert_eq!(unlocked.summary, "short summary");
        assert_eq!(unlocked.body, "body preserved in commentary metadata");
    }

    #[test]
    fn asset_notes_refuse_lock() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut picture = book.new_note(ObjectType::Picture, "photo").unwrap();
        picture.summary = "caption".into();
        book.save_note(&picture).unwrap();
        assert!(set_note_pin_locked(&book, picture.id.as_str(), true, &key).is_err());
    }

    #[test]
    fn change_book_pin_reencrypts_locked_notes_under_new_key() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut note = book.new_note(ObjectType::Note, "diary").unwrap();
        note.body = "secret".into();
        book.save_note(&note).unwrap();
        set_note_pin_locked(&book, note.id.as_str(), true, &key).unwrap();

        let new_key = change_book_pin(&book, "1234", "5678", Some("new hint".into())).unwrap();
        assert_ne!(new_key.key_id(), key.key_id());
        assert_eq!(pin_hint(&book).unwrap().as_deref(), Some("new hint"));

        // Old key no longer verifies; old PIN is rejected outright.
        assert!(unlock_book(&book, "1234").is_err());
        let reverified = unlock_book(&book, "5678").unwrap();
        assert_eq!(reverified.key_id(), new_key.key_id());

        let stored = book.store.read_note(&note.id).unwrap();
        assert_eq!(stored.encryption.as_ref().unwrap().key_id, new_key.key_id());
        assert_eq!(
            pinlock::decrypt_note(&stored, &new_key).unwrap().body,
            "secret"
        );
    }

    #[test]
    fn remove_book_pin_unlocks_every_note_and_deletes_keycheck() {
        let (_d, book) = book();
        let key = set_book_pin(&book, "1234", None).unwrap();
        let mut a = book.new_note(ObjectType::Note, "a").unwrap();
        a.body = "secret a".into();
        book.save_note(&a).unwrap();
        set_note_pin_locked(&book, a.id.as_str(), true, &key).unwrap();
        let mut b = book.new_note(ObjectType::Note, "b").unwrap();
        b.body = "secret b".into();
        book.save_note(&b).unwrap();
        set_note_pin_locked(&book, b.id.as_str(), true, &key).unwrap();

        remove_book_pin(&book, &key).unwrap();

        assert!(!is_pin_configured(&book));
        assert_eq!(book.store.read_note(&a.id).unwrap().body, "secret a");
        assert_eq!(book.store.read_note(&b.id).unwrap().body, "secret b");
        assert!(!book.store.read_note(&a.id).unwrap().is_pin_locked());
    }
}
