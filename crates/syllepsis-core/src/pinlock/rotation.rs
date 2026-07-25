//! Crash-safe PIN rotation: swap a book's keycheck and re-encrypt every locked note under the new
//! key, or leave the book exactly as it was.
//!
//! Rotation is the one PIN operation that can lose notes, because it touches the keycheck *and*
//! every locked note's ciphertext. Two failure windows have to be closed explicitly:
//!
//! 1. **Zero-keycheck window** — deleting the old `_pinlock.json` before writing the new one means
//!    a crash in between leaves a book full of ciphertext and no way to derive any key. Closed by
//!    overwriting through [`write_atomic`] (temp file + rename), so the path always names either
//!    the old keycheck or the new one.
//! 2. **Mixed-key window** — failing partway through re-encryption leaves some notes under the old
//!    key and some under the new, and no single PIN opens the whole book. Closed by decrypting
//!    *every* locked note up front (aborting before any write if one of them can't be read) and
//!    holding the plaintext in memory, so any later failure can be rolled back: old keycheck bytes
//!    restored, already-converted notes re-encrypted back under the old key.
//!
//! Plaintext held for the rollback is the sensitive part of that design, so it lives in
//! [`Zeroizing`] buffers that wipe themselves when the rotation ends either way.

use zeroize::Zeroizing;

use super::BookKey;
use crate::error::{CoreError, CoreResult};
use crate::model::Note;
use crate::storage::atomic::write_atomic;
use crate::storage::{layout, Book, NoteStore};

/// How a locked note is turned back into plaintext during pre-flight. Injected (rather than called
/// directly) because the caller that owns recovery-from-plaintext-artifacts lives in the `app`
/// layer above this module; see `app::pinlock::decrypt_note_with_recovery`.
pub type LockedNoteDecryptor = fn(&Book, &Note, &BookKey) -> CoreResult<Note>;

/// A locked note's pre-flight state: the note exactly as stored under the old key, plus the
/// plaintext read out of it. The plaintext is what makes rollback possible at all — once a note has
/// been rewritten under the new key its old ciphertext is gone from disk, so re-encrypting from
/// plaintext we still hold is the only way back.
struct LockedNoteRotationPlan {
    stored_note: Note,
    plaintext_summary: Zeroizing<String>,
    plaintext_body: Zeroizing<String>,
}

/// Verify `old_pin`, mint a keycheck for `new_pin`, and re-encrypt every locked note under the new
/// key — atomically, in the sense that a failure anywhere leaves the book fully usable under
/// `old_pin`. Returns the new book key.
pub fn rotate_book_pin(
    book: &Book,
    old_pin: &str,
    new_pin: &str,
    hint: Option<String>,
    decrypt_locked_note: LockedNoteDecryptor,
) -> CoreResult<BookKey> {
    rotate_book_pin_observing_each_reencrypt(
        book,
        old_pin,
        new_pin,
        hint,
        decrypt_locked_note,
        &|_| Ok(()),
    )
}

/// [`rotate_book_pin`] with a seam for tests: `observe_before_note_reencrypt` runs immediately
/// before each note is rewritten under the new key, and an `Err` from it is treated exactly like a
/// re-encryption failure. That makes the rollback path — the whole point of this module —
/// reachable without corrupting a real note or filling a real disk.
pub(crate) fn rotate_book_pin_observing_each_reencrypt(
    book: &Book,
    old_pin: &str,
    new_pin: &str,
    hint: Option<String>,
    decrypt_locked_note: LockedNoteDecryptor,
    observe_before_note_reencrypt: &dyn Fn(&Note) -> CoreResult<()>,
) -> CoreResult<BookKey> {
    let old_key = super::verify_pin(&book.root, old_pin)?;
    // Snapshot before anything is touched: these bytes are the only way back to the old PIN.
    let old_keycheck_bytes = std::fs::read(layout::pinlock_path(&book.root))?;

    let rotation_plans =
        build_rotation_plan_for_every_locked_note(book, &old_key, decrypt_locked_note)?;

    // Overwrite rather than remove-then-create, so the keycheck path is never absent.
    let new_key = super::create_pinlock(&book.root, new_pin, hint)?;

    for (converted_count, plan) in rotation_plans.iter().enumerate() {
        let reencrypt_result = observe_before_note_reencrypt(&plan.stored_note)
            .and_then(|()| save_note_encrypted_under_key(book, plan, &new_key));
        if let Err(failure) = reencrypt_result {
            let rollback = roll_back_partial_rotation(
                book,
                &old_keycheck_bytes,
                &rotation_plans[..converted_count],
                &old_key,
            );
            return Err(describe_rotation_failure(
                &plan.stored_note,
                failure,
                rollback,
            ));
        }
    }

    Ok(new_key)
}

/// Pre-flight: decrypt-verify every locked note under the old key *before* any write happens. A
/// note that cannot be read is a hard abort — better to refuse the PIN change outright than to
/// discover the unreadable note halfway through, when the old ciphertext of its predecessors has
/// already been replaced.
fn build_rotation_plan_for_every_locked_note(
    book: &Book,
    old_key: &BookKey,
    decrypt_locked_note: LockedNoteDecryptor,
) -> CoreResult<Vec<LockedNoteRotationPlan>> {
    let mut rotation_plans = Vec::new();
    for stored_note in book.store.read_all_notes()? {
        if !stored_note.is_pin_locked() {
            continue;
        }
        let plain = decrypt_locked_note(book, &stored_note, old_key).map_err(|error| {
            CoreError::PinLock(format!(
                "note {} (\"{}\") could not be decrypted while changing the PIN: {error}",
                stored_note.id, stored_note.title
            ))
        })?;
        rotation_plans.push(LockedNoteRotationPlan {
            // Move (not clone) the plaintext into zeroizing buffers so no un-wiped copy is left
            // behind in the decrypted note.
            plaintext_summary: Zeroizing::new(plain.summary),
            plaintext_body: Zeroizing::new(plain.body),
            stored_note,
        });
    }
    Ok(rotation_plans)
}

/// Write the planned note encrypted under `key`, from the plaintext held in the plan.
fn save_note_encrypted_under_key(
    book: &Book,
    plan: &LockedNoteRotationPlan,
    key: &BookKey,
) -> CoreResult<()> {
    let mut note = plan.stored_note.clone();
    note.summary = plan.plaintext_summary.as_str().to_string();
    note.body = plan.plaintext_body.as_str().to_string();
    // `encrypt_note` refuses an already-locked note; the plan carries the pre-rotation metadata.
    note.encryption = None;
    super::encrypt_note(&mut note, key)?;
    book.save_note(&note)
}

/// Undo a rotation that failed partway through. Keycheck first: even if a note write then fails,
/// the book is at least openable under the old PIN, which is what the user will try.
fn roll_back_partial_rotation(
    book: &Book,
    old_keycheck_bytes: &[u8],
    already_converted: &[LockedNoteRotationPlan],
    old_key: &BookKey,
) -> CoreResult<()> {
    write_atomic(&layout::pinlock_path(&book.root), old_keycheck_bytes)?;
    for plan in already_converted {
        save_note_encrypted_under_key(book, plan, old_key)?;
    }
    Ok(())
}

/// Name the note that broke the rotation and say plainly whether the book is back on the old PIN,
/// because that determines what the user should do next (retry vs. restore a backup).
fn describe_rotation_failure(
    note: &Note,
    failure: CoreError,
    rollback: CoreResult<()>,
) -> CoreError {
    match rollback {
        Ok(()) => CoreError::PinLock(format!(
            "PIN change aborted while re-encrypting note {} (\"{}\"): {failure}. \
             The book was rolled back and still opens with the old PIN.",
            note.id, note.title
        )),
        Err(rollback_error) => CoreError::PinLock(format!(
            "PIN change failed while re-encrypting note {} (\"{}\"): {failure}. \
             Rolling back to the old PIN also failed: {rollback_error}",
            note.id, note.title
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::app::pinlock::{
        change_book_pin, decrypt_note_with_recovery, pin_hint, set_book_pin, set_note_pin_locked,
        unlock_book,
    };
    use crate::id::NoteId;
    use crate::model::ObjectType;

    /// A book with `bodies.len()` locked notes, plus the ids in creation order.
    fn book_with_locked_notes(bodies: &[&str]) -> (tempfile::TempDir, Book, Vec<NoteId>) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        let key = set_book_pin(&book, "1234", Some("old hint".to_string())).unwrap();
        let mut ids = Vec::new();
        for (index, body) in bodies.iter().enumerate() {
            let mut note = book
                .new_note(ObjectType::Note, &format!("note {index}"))
                .unwrap();
            note.summary = format!("summary {index}");
            note.body = (*body).to_string();
            book.save_note(&note).unwrap();
            set_note_pin_locked(&book, note.id.as_str(), true, &key).unwrap();
            ids.push(note.id);
        }
        (dir, book, ids)
    }

    /// Every markdown note plus the keycheck, by path, for byte-identity assertions.
    fn snapshot_notes_and_keycheck(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        collect_note_and_keycheck_bytes(root, &mut snapshot);
        snapshot
    }

    fn collect_note_and_keycheck_bytes(dir: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_note_and_keycheck_bytes(&path, into);
                continue;
            }
            let is_note = path.extension().is_some_and(|ext| ext == "md");
            let is_keycheck = path.file_name().is_some_and(|name| name == "_pinlock.json");
            if is_note || is_keycheck {
                into.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }

    #[test]
    fn rotation_rolls_the_whole_book_back_to_the_old_pin_when_a_note_fails_midway() {
        let bodies = ["secret zero", "secret one", "secret two"];
        let (_d, book, ids) = book_with_locked_notes(&bodies);
        let old_key = unlock_book(&book, "1234").unwrap();

        // Fail on the second note, so the first is already converted and must be rolled back.
        let attempts = AtomicUsize::new(0);
        let error = rotate_book_pin_observing_each_reencrypt(
            &book,
            "1234",
            "5678",
            Some("new hint".to_string()),
            decrypt_note_with_recovery,
            &|_note| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 1 {
                    Err(CoreError::PinLock(
                        "injected re-encrypt failure".to_string(),
                    ))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("injected failure must abort the rotation");
        assert!(error.to_string().contains("rolled back"), "{error}");

        // The old PIN still opens the book, and the new one was never adopted.
        let reverified = unlock_book(&book, "1234").unwrap();
        assert_eq!(reverified.key_id(), old_key.key_id());
        assert!(unlock_book(&book, "5678").is_err());
        assert_eq!(pin_hint(&book).unwrap().as_deref(), Some("old hint"));

        // Every note — including the one already converted before the failure — reads under it.
        for (id, expected_body) in ids.iter().zip(bodies) {
            let stored = book.store.read_note(id).unwrap();
            assert!(stored.is_pin_locked());
            assert_eq!(stored.encryption.as_ref().unwrap().key_id, old_key.key_id());
            let plain = super::super::decrypt_note(&stored, &reverified).unwrap();
            assert_eq!(plain.body, expected_body);
        }
    }

    #[test]
    fn preflight_failure_leaves_the_keycheck_and_every_note_byte_identical() {
        let (_d, book, ids) = book_with_locked_notes(&["readable secret", "unreadable secret"]);

        // Corrupt the second note's *summary* ciphertext: plaintext-artifact recovery can only
        // rebuild a body, so this is genuinely undecryptable and must abort the pre-flight.
        let mut broken = book.store.read_note(&ids[1]).unwrap();
        broken.summary.push('A');
        book.save_note(&broken).unwrap();

        let before = snapshot_notes_and_keycheck(&book.root);
        assert!(before.len() >= 3, "expected 2 notes + keycheck: {before:?}");

        let error = change_book_pin(&book, "1234", "5678", Some("new hint".to_string()))
            .expect_err("an undecryptable locked note must abort the PIN change");
        assert!(
            error.to_string().contains("could not be decrypted"),
            "{error}"
        );

        assert_eq!(snapshot_notes_and_keycheck(&book.root), before);
        assert!(unlock_book(&book, "1234").is_ok());
        assert!(unlock_book(&book, "5678").is_err());
    }

    #[test]
    fn rotation_never_leaves_the_book_without_a_keycheck() {
        let (_d, book, _ids) = book_with_locked_notes(&["secret"]);
        let keycheck_path = layout::pinlock_path(&book.root);

        // The observer runs between the new keycheck landing and the first note rewrite — the exact
        // window the old remove-then-create sequence left empty.
        rotate_book_pin_observing_each_reencrypt(
            &book,
            "1234",
            "5678",
            None,
            decrypt_note_with_recovery,
            &|_note| {
                assert!(keycheck_path.is_file(), "keycheck vanished mid-rotation");
                Ok(())
            },
        )
        .unwrap();
        assert!(keycheck_path.is_file());
    }
}
