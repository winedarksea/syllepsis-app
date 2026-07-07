//! Commands for PIN-locked notes (privacy-security.md "PIN-Locked Notes").
//!
//! The book-wide keycheck and per-note lock/unlock hygiene live in `syllepsis_core::app::pinlock`;
//! this module owns what's inherently process/UI state: the in-memory [`crate::pin_session`] (which
//! key, if any, is currently held) and the OS-keychain "remember on this device" vault entry.

use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use syllepsis_core::app::dto::NoteDto;
use syllepsis_core::app::pinlock as app;
use syllepsis_core::pinlock::BookKey;
use syllepsis_core::storage::{Book, NoteStore};

use crate::pin_session::PinSession;
use crate::secrets::{self, KeyringVaultStore, StoredBookKey};
use crate::state::AppState;

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// Event emitted whenever the session's unlocked/locked state changes (unlock, manual lock, idle
/// auto-relock, PIN change/removal) so the UI can refresh without polling.
pub const PIN_SESSION_EVENT: &str = "pin-session-changed";

fn emit_session_changed(app: &AppHandle) {
    let _ = app.emit(PIN_SESSION_EVENT, ());
}

const RELOCK_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Background poll started once from `lib.rs` setup: relocks an idle session past the book's
/// configured `pin_idle_relock_minutes` and notifies the UI. A book with no PIN configured (or no
/// book open at all) is simply never unlocked, so this is a no-op until the user actually unlocks.
pub fn start_pin_session_relock_poll(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(RELOCK_POLL_INTERVAL);
        let state = app.state::<AppState>();
        let idle_minutes = {
            let guard = state.book.lock().unwrap();
            guard
                .as_ref()
                .map(|book| book.config.privacy.pin_idle_relock_minutes)
        };
        let Some(idle_minutes) = idle_minutes else {
            continue;
        };
        let relocked = state
            .pin_session
            .lock()
            .unwrap()
            .relock_if_idle(SystemTime::now(), idle_minutes);
        if relocked {
            emit_session_changed(&app);
        }
    });
}

/// Everything the settings card / unlock modal need to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinLockStatus {
    pub configured: bool,
    pub hint: Option<String>,
    pub unlocked: bool,
    pub idle_relock_minutes: u32,
    /// Whether "unlock with device credential" is available for this book on this device.
    pub remembered_key_available: bool,
}

fn status(state: &State<AppState>) -> Result<PinLockStatus, String> {
    with_book!(state, book, {
        let remembered_key_available = {
            let _guard = state.secrets_lock.lock().unwrap();
            let mut store = KeyringVaultStore::new();
            secrets::read_pinlock_key(&mut store, &book.metadata.book_id)?.is_some()
        };
        Ok(PinLockStatus {
            configured: app::is_pin_configured(book),
            hint: app::pin_hint(book).map_err(|e| e.to_string())?,
            unlocked: state.pin_session.lock().unwrap().is_unlocked(),
            idle_relock_minutes: book.config.privacy.pin_idle_relock_minutes,
            remembered_key_available,
        })
    })
}

/// The session's current status (for the settings card and the unlock modal).
#[tauri::command]
pub fn get_pin_lock_status(state: State<AppState>) -> Result<PinLockStatus, String> {
    status(&state)
}

/// Set the book's PIN for the first time.
#[tauri::command]
pub fn set_book_pin(
    app: AppHandle,
    state: State<AppState>,
    pin: String,
    hint: Option<String>,
) -> Result<PinLockStatus, String> {
    let key =
        with_book!(state, book, { app::set_book_pin(book, &pin, hint).map_err(|e| e.to_string()) })?;
    state.pin_session.lock().unwrap().unlock(key, SystemTime::now());
    emit_session_changed(&app);
    status(&state)
}

/// Verify `pin` and unlock the session, optionally remembering the key on this device.
#[tauri::command]
pub fn unlock_book(
    app: AppHandle,
    state: State<AppState>,
    pin: String,
    remember: bool,
) -> Result<PinLockStatus, String> {
    let (key, book_id) = with_book!(state, book, {
        let key = app::unlock_book(book, &pin).map_err(|e| e.to_string())?;
        Ok::<_, String>((key, book.metadata.book_id.clone()))
    })?;
    if remember {
        remember_key(&state, &book_id, &key)?;
    }
    state.pin_session.lock().unwrap().unlock(key, SystemTime::now());
    emit_session_changed(&app);
    status(&state)
}

/// Unlock using a previously-remembered device key (desktop only — see
/// [`crate::secrets::StoredBookKey`]). The caller (frontend) is responsible for gating this behind
/// an OS biometric/device-credential prompt (`tauri-plugin-biometric`) before invoking it; this
/// command's own job is just validating the remembered key still matches the book's current
/// keycheck (it won't if the PIN was since changed/removed elsewhere).
#[tauri::command]
pub fn unlock_book_with_device_credential(
    app: AppHandle,
    state: State<AppState>,
) -> Result<PinLockStatus, String> {
    let book_id = with_book!(state, book, {
        Ok::<_, String>(book.metadata.book_id.clone())
    })?;
    let stored = {
        let _guard = state.secrets_lock.lock().unwrap();
        let mut store = KeyringVaultStore::new();
        secrets::read_pinlock_key(&mut store, &book_id)?
    }
    .ok_or_else(|| "no remembered key for this book on this device".to_string())?;

    let key_bytes: [u8; 32] = B64
        .decode(&stored.key_b64)
        .map_err(|e| format!("corrupt remembered key: {e}"))?
        .try_into()
        .map_err(|_| "corrupt remembered key: wrong length".to_string())?;
    let key = BookKey::new(key_bytes, stored.key_id.clone());

    with_book!(state, book, {
        let current = syllepsis_core::pinlock::load_pinlock(&book.root).map_err(|e| e.to_string())?;
        if current.key_id != stored.key_id {
            return Err("the book's PIN changed; the remembered key is stale".to_string());
        }
        Ok::<(), String>(())
    })?;

    state.pin_session.lock().unwrap().unlock(key, SystemTime::now());
    emit_session_changed(&app);
    status(&state)
}

fn remember_key(state: &State<AppState>, book_id: &str, key: &BookKey) -> Result<(), String> {
    let _guard = state.secrets_lock.lock().unwrap();
    let mut store = KeyringVaultStore::new();
    secrets::write_pinlock_key(
        &mut store,
        book_id,
        StoredBookKey {
            key_b64: B64.encode(key.expose_for_vault_storage()),
            key_id: key.key_id().to_string(),
        },
    )
}

fn forget_remembered_key(state: &State<AppState>, book_id: &str) -> Result<(), String> {
    let _guard = state.secrets_lock.lock().unwrap();
    let mut store = KeyringVaultStore::new();
    secrets::delete_pinlock_key(&mut store, book_id)
}

/// Manually relock the session (the explicit "Lock now" button).
#[tauri::command]
pub fn lock_book_now(app: AppHandle, state: State<AppState>) -> Result<PinLockStatus, String> {
    state.pin_session.lock().unwrap().lock();
    emit_session_changed(&app);
    status(&state)
}

/// Change the book's PIN (requires the old one). Re-encrypts every locked note under the new key
/// and drops any remembered device key, since it no longer matches.
#[tauri::command]
pub fn change_book_pin(
    app: AppHandle,
    state: State<AppState>,
    old_pin: String,
    new_pin: String,
    hint: Option<String>,
) -> Result<PinLockStatus, String> {
    let (key, book_id) = with_book!(state, book, {
        let key = app::change_book_pin(book, &old_pin, &new_pin, hint).map_err(|e| e.to_string())?;
        Ok::<_, String>((key, book.metadata.book_id.clone()))
    })?;
    let _ = forget_remembered_key(&state, &book_id);
    state.pin_session.lock().unwrap().unlock(key, SystemTime::now());
    emit_session_changed(&app);
    status(&state)
}

/// Remove the book's PIN entirely (requires the current PIN): every locked note is decrypted back
/// to plaintext, the keycheck is deleted, and any remembered device key is forgotten.
#[tauri::command]
pub fn remove_book_pin(
    app: AppHandle,
    state: State<AppState>,
    pin: String,
) -> Result<PinLockStatus, String> {
    let book_id = with_book!(state, book, {
        let key = app::unlock_book(book, &pin).map_err(|e| e.to_string())?;
        app::remove_book_pin(book, &key).map_err(|e| e.to_string())?;
        Ok::<_, String>(book.metadata.book_id.clone())
    })?;
    let _ = forget_remembered_key(&state, &book_id);
    state.pin_session.lock().unwrap().lock();
    emit_session_changed(&app);
    status(&state)
}

/// Update the PIN hint without changing the PIN.
#[tauri::command]
pub fn set_pin_hint(state: State<AppState>, hint: Option<String>) -> Result<PinLockStatus, String> {
    with_book!(state, book, {
        app::set_pin_hint(book, hint).map_err(|e| e.to_string())
    })?;
    status(&state)
}

/// Decrypt-on-read at the tauri boundary: the one sanctioned bypass of `NoteDto::from_note`'s
/// fail-safe blanking. If `dto` isn't PIN-locked, or the session has no key, it's returned
/// unchanged (still blanked when locked — core itself never leaks past that). A decrypt failure
/// (tampered/foreign-key ciphertext) also falls back to the blanked DTO rather than erroring the
/// whole read — the affected note simply keeps showing its locked placeholder.
///
/// Touches the session's idle clock on a successful decrypt: viewing/editing unlocked content is
/// activity, same as the explicit note commands.
pub fn present(book: &Book, dto: NoteDto, session: &mut PinSession) -> NoteDto {
    if !dto.pin_locked {
        return dto;
    }
    let Some(key) = session.key().cloned() else {
        return dto;
    };
    let Ok(note_id) = syllepsis_core::id::NoteId::parse(&dto.id) else {
        return dto;
    };
    let Ok(note) = book.store.read_note(&note_id) else {
        return dto;
    };
    match syllepsis_core::pinlock::decrypt_note(&note, &key) {
        Ok(plain) => {
            session.touch(SystemTime::now());
            NoteDto::from_decrypted(&note, plain.summary, plain.body)
        }
        Err(_) => dto,
    }
}

/// Lock or unlock a single note. Requires an unlocked session (a typed error the frontend maps to
/// the unlock modal otherwise).
#[tauri::command]
pub fn set_note_pin_locked(
    state: State<AppState>,
    id: String,
    locked: bool,
) -> Result<NoteDto, String> {
    let key = state
        .pin_session
        .lock()
        .unwrap()
        .key()
        .cloned()
        .ok_or_else(|| "book is locked".to_string())?;
    with_book!(state, book, {
        let dto = app::set_note_pin_locked(book, &id, locked, &key).map_err(|e| e.to_string())?;
        state.invalidate_graph_corpus();
        Ok(dto)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syllepsis_core::app::commands::{create_note, update_note};
    use syllepsis_core::model::ObjectType;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    fn locked_note(book: &Book) -> String {
        let key = app::set_book_pin(book, "1234", None).unwrap();
        let mut dto = create_note(book, ObjectType::Note, "Diary", None).unwrap();
        dto.body = "secret content".to_string();
        let saved = update_note(book, dto).unwrap();
        app::set_note_pin_locked(book, &saved.id, true, &key).unwrap();
        saved.id
    }

    #[test]
    fn present_returns_blanked_dto_when_session_is_locked() {
        let (_d, book) = book();
        let id = locked_note(&book);
        let dto = syllepsis_core::app::commands::get_note(&book, &id).unwrap();
        assert!(dto.pin_locked && !dto.unlocked && dto.body.is_empty());

        let mut session = PinSession::new();
        let presented = present(&book, dto, &mut session);
        assert!(presented.pin_locked);
        assert!(!presented.unlocked);
        assert!(presented.body.is_empty());
    }

    #[test]
    fn present_decrypts_and_touches_session_when_unlocked() {
        let (_d, book) = book();
        let id = locked_note(&book);
        let dto = syllepsis_core::app::commands::get_note(&book, &id).unwrap();

        let mut session = PinSession::new();
        let keycheck = syllepsis_core::pinlock::load_pinlock(&book.root).unwrap();
        let key = syllepsis_core::pinlock::keycheck::derive_key(&keycheck, "1234").unwrap();
        session.unlock(key, SystemTime::UNIX_EPOCH);

        let presented = present(&book, dto, &mut session);
        assert!(presented.pin_locked);
        assert!(presented.unlocked);
        assert_eq!(presented.body, "secret content");
        // Viewing decrypted content is activity: the idle clock moved off the unlock instant.
        assert!(!session.relock_if_idle(SystemTime::UNIX_EPOCH, 0));
    }

    #[test]
    fn present_falls_back_to_blanked_on_wrong_key() {
        let (_d, book) = book();
        let id = locked_note(&book);
        let dto = syllepsis_core::app::commands::get_note(&book, &id).unwrap();

        let mut session = PinSession::new();
        session.unlock(BookKey::new([9u8; 32], "wrongkey".to_string()), SystemTime::now());

        let presented = present(&book, dto, &mut session);
        assert!(!presented.unlocked);
        assert!(presented.body.is_empty());
    }

    #[test]
    fn present_is_a_noop_for_unlocked_notes() {
        let (_d, book) = book();
        let dto = create_note(&book, ObjectType::Note, "Plain", None).unwrap();
        let mut session = PinSession::new();
        let presented = present(&book, dto.clone(), &mut session);
        assert_eq!(presented, dto);
    }
}
