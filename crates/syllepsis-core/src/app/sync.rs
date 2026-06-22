//! Application command surface for Phase 4 sync. Framework-agnostic operations over a [`Book`]
//! that the Tauri shell wraps as commands (and a PWA worker can call directly).
//!
//! Only the local/mounted-folder provider is wired in-core today (the cloud HTTP providers are
//! declared in [`provider_descriptors`] for the UI roadmap), so the one action here runs a sync
//! pass against a folder — which is exactly how the Google Drive / Dropbox desktop clients expose
//! the cloud, so it is real sync, not a placeholder.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::storage::Book;
use crate::sync::{self, LocalFolderSync, SyncEngine, SyncProviderDescriptor, SyncReport};

/// A snapshot of the book's sync configuration for the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusDto {
    /// Whether sync is enabled in config.
    pub enabled: bool,
    /// The selected CRDT backend (`lww` or `loro`).
    pub crdt_backend: String,
    /// This device's stable actor id (for diagnostics / "this device" labeling).
    pub actor_id: String,
    /// Every sync target the app advertises, with honest `implemented` flags.
    pub providers: Vec<SyncProviderDescriptor>,
}

/// The sync targets the app knows how to offer.
pub fn provider_descriptors() -> Vec<SyncProviderDescriptor> {
    sync::provider_descriptors()
}

/// Report this book's sync configuration and this device's identity.
pub fn sync_status(book: &Book) -> CoreResult<SyncStatusDto> {
    Ok(SyncStatusDto {
        enabled: book.config.sync.enabled,
        crdt_backend: book.config.sync.crdt_backend.clone(),
        actor_id: sync::actor_id_for(&book.root)?.as_str().to_string(),
        providers: provider_descriptors(),
    })
}

/// Run one sync pass against a local/mounted folder at `remote_path` (a cloud-drive mount, an
/// external disk, or a plain directory). Refreshes the note store afterward so pulled notes are
/// immediately visible.
pub fn sync_to_local_folder(book: &Book, remote_path: &str) -> CoreResult<SyncReport> {
    if !book.config.sync.enabled {
        return Err(CoreError::Sync(
            "sync is disabled in this book's config".into(),
        ));
    }
    let provider = Box::new(LocalFolderSync::open(remote_path)?);
    let actor = sync::actor_id_for(&book.root)?;
    let report = SyncEngine::new(book.root.clone(), provider, actor, &book.config.sync).sync()?;
    // A pull may have written new note files; rebuild the id index so they resolve.
    book.store.refresh()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;
    use crate::storage::NoteStore;

    fn book_at(path: std::path::PathBuf) -> Book {
        Book::create(path, "Shared").unwrap()
    }

    #[test]
    fn status_reports_config_and_a_stable_actor() {
        let dir = tempfile::tempdir().unwrap();
        let book = book_at(dir.path().join("b"));
        let status = sync_status(&book).unwrap();
        assert!(status.enabled);
        assert_eq!(status.crdt_backend, crate::crdt::LWW_BACKEND);
        assert!(!status.actor_id.is_empty());
        assert!(status.providers.iter().any(|p| p.implemented));
    }

    #[test]
    fn sync_round_trips_a_note_between_two_books_via_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("remote");
        let remote = remote.to_str().unwrap();
        let a = book_at(dir.path().join("a"));
        let b = book_at(dir.path().join("b"));

        let note = a.new_note(ObjectType::Note, "wiring").unwrap();
        let mut n = note.clone();
        n.body = "panel".into();
        a.save_note(&n).unwrap();

        sync_to_local_folder(&a, remote).unwrap();
        let report = sync_to_local_folder(&b, remote).unwrap();
        assert!(report.pulled.iter().any(|p| p.ends_with(".md")));
        assert_eq!(b.store.read_note(&note.id).unwrap().body, "panel");
    }

    #[test]
    fn sync_errors_clearly_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = book_at(dir.path().join("b"));
        book.config.sync.enabled = false;
        let err = sync_to_local_folder(&book, dir.path().join("r").to_str().unwrap()).unwrap_err();
        assert!(matches!(err, CoreError::Sync(_)));
    }
}
