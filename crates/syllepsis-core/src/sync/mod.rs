//! Phase 4 sync: CRDT-backed, last-write-safe synchronization of a book to a user-owned remote.
//!
//! The pieces fit together as:
//! - [`crdt`](crate::crdt) — per-note convergent documents (the merge layer).
//! - [`SyncProvider`] — the remote-store seam ([`LocalFolderSync`] is the default).
//! - [`SyncState`](state::SyncState) — this device's last-sync fingerprints (loop prevention).
//! - [`plan`](plan::plan) — the pure who-changed-what decision matrix.
//! - [`SyncEngine`] — the orchestration that reconciles markdown ⇄ sidecars, runs the plan against
//!   the provider, and merges concurrent edits.
//!
//! Design rules realized here (sync-backup.md, platform-infra.md): markdown is the source of truth
//! for *local* edits; per-note CRDT sidecars merge across devices; binary assets are tracked by
//! UUID sidecars rather than CRDT'd; and conflicts on non-mergeable files become explicit
//! `.conflict-*` copies the user resolves.

mod assets;
mod engine;
mod local_folder;
mod plan;
mod provider;
mod state;

pub use assets::{assign as assign_asset_uuid, AssetRegistry, AssetSidecar};
pub use engine::{SyncEngine, SyncReport};
pub use local_folder::{content_revision, LocalFolderSync};
pub use plan::{plan, SyncAction};
pub use provider::{
    provider_descriptors, RemoteEntry, RemoteRevision, SyncProvider, SyncProviderDescriptor,
    SyncProviderKind, GITHUB_ID, GOOGLE_DRIVE_ID, LOCAL_FOLDER_ID,
};
pub use state::{SyncState, SyncedFile};

use std::path::Path;

use crate::crdt::ActorId;
use crate::error::CoreResult;
use crate::id::NoteId;
use crate::storage::layout;

/// File name (under `_sync/`) holding this device's persistent actor id.
const ACTOR_FILE: &str = "actor-id";

/// Resolve this device's actor id, minting and persisting one under `_sync/` on first call. Every
/// device must have a *distinct, stable* actor so CRDT tie-breaks are deterministic — so it is
/// device-local (never synced) and survives restarts.
pub fn actor_id_for(book_root: &Path) -> CoreResult<ActorId> {
    let path = layout::sync_dir(book_root).join(ACTOR_FILE);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(ActorId::new(trimmed));
        }
    }
    let actor = ActorId::generate();
    std::fs::create_dir_all(layout::sync_dir(book_root))?;
    std::fs::write(&path, actor.as_str())?;
    Ok(actor)
}

/// True if a book-relative path is a note markdown file — the only kind synced through a CRDT
/// sidecar. The test is the id scheme itself: a note's filename stem is its `{type}-{slug}-{ulid}`
/// id, so it parses as a [`NoteId`]; category files (`_categories/x.md`), `_book.md`, and conflict
/// copies do not, and are synced as plain files.
pub fn is_note_md(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".md"))
        .map(|stem| NoteId::parse(stem).is_ok())
        .unwrap_or(false)
}

/// The CRDT sidecar's book-relative path for a note markdown path, or `None` if the path is not a
/// note. Keyed on the ulid, so it is independent of the note's slug/folder.
pub fn sidecar_rel_path(note_path: &str) -> Option<String> {
    let stem = note_path.rsplit('/').next()?.strip_suffix(".md")?;
    let id = NoteId::parse(stem).ok()?;
    Some(format!(
        "{}/{}.{}",
        layout::CRDT_DIR,
        id.ulid(),
        layout::CRDT_EXTENSION
    ))
}

/// True if a book-relative path is device-local bookkeeping or an ephemeral cache that must never
/// be synced (`_sync/`, `_derived/`).
pub fn is_local_only(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or("");
    first == layout::SYNC_DIR || first == layout::DERIVED_DIR
}

/// True if a path is a CRDT sidecar (`_crdt/*.crdt`). Sidecars are synced, but as *dependents* of
/// their note rather than as independently-planned files, so the planner skips them.
pub fn is_sidecar(path: &str) -> bool {
    path.starts_with(&format!("{}/", layout::CRDT_DIR))
        && path.ends_with(&format!(".{}", layout::CRDT_EXTENSION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    #[test]
    fn note_files_are_detected_by_the_id_scheme() {
        let id = NoteId::generate(ObjectType::Note.id_prefix(), "kitchen wiring");
        let note_path = format!("{}.md", id.as_str());
        assert!(is_note_md(&note_path));
        assert!(is_note_md(&format!("subdir/{note_path}")));
        // Non-notes:
        assert!(!is_note_md("_categories/electrical.md"));
        assert!(!is_note_md("_book.md"));
        assert!(!is_note_md("note-x.conflict-ab12.md"));
    }

    #[test]
    fn sidecar_path_is_keyed_on_the_ulid() {
        let id = NoteId::generate(ObjectType::Note.id_prefix(), "title here");
        let note_path = format!("{}.md", id.as_str());
        let sidecar = sidecar_rel_path(&note_path).unwrap();
        assert_eq!(sidecar, format!("_crdt/{}.crdt", id.ulid()));
        assert!(is_sidecar(&sidecar));
        assert!(sidecar_rel_path("_book.md").is_none());
    }

    #[test]
    fn actor_id_is_stable_across_calls() {
        let dir = tempfile::tempdir().unwrap();
        let a = actor_id_for(dir.path()).unwrap();
        let b = actor_id_for(dir.path()).unwrap();
        assert_eq!(a.as_str(), b.as_str());
    }
}
