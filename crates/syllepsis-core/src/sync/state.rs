//! [`SyncState`]: this device's memory of the last successful sync with one remote.
//!
//! For every synced path it records two things: the local content hash we last reconciled, and the
//! remote revision we last saw. Together they let the engine classify each path on the next pass:
//! - local hash differs from disk  ⇒ changed here.
//! - remote revision differs from the remote's list ⇒ changed there.
//! - neither differs ⇒ **skip** — the single most important loop-prevention rule, since a file we
//!   do not touch cannot trigger another sync.
//!
//! State is device-local (it describes *this* machine's view of the remote) so it lives under
//! `_sync/` and is never itself synced or committed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::storage::layout;
use crate::sync::provider::RemoteRevision;

/// What we knew about one path at the end of the last sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedFile {
    /// Content hash (see [`content_revision`](super::local_folder::content_revision)) of the local
    /// bytes the last time this path was reconciled with the remote.
    pub local_hash: String,
    /// The remote revision last observed for this path.
    pub remote_revision: RemoteRevision,
}

/// Per-provider sync bookkeeping for one book. Serialized as JSON under `_sync/`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    /// The provider these records belong to (different remotes keep independent state).
    pub provider: String,
    /// path → last-synced fingerprint.
    pub files: BTreeMap<String, SyncedFile>,
}

impl SyncState {
    pub fn new(provider: impl Into<String>) -> SyncState {
        SyncState {
            provider: provider.into(),
            files: BTreeMap::new(),
        }
    }

    /// Load the state for `provider` from `_sync/`, or a fresh empty one on first sync.
    pub fn load(book_root: &Path, provider: &str) -> CoreResult<SyncState> {
        let path = state_path(book_root, provider);
        if !path.exists() {
            return Ok(SyncState::new(provider));
        }
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Persist this state under `_sync/` (creating the directory if needed).
    pub fn save(&self, book_root: &Path) -> CoreResult<()> {
        let dir = layout::sync_dir(book_root);
        std::fs::create_dir_all(&dir)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(state_path(book_root, &self.provider), bytes)?;
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&SyncedFile> {
        self.files.get(path)
    }

    /// Record that `path` is now in sync at the given local hash and remote revision.
    pub fn mark_synced(
        &mut self,
        path: &str,
        local_hash: impl Into<String>,
        remote_revision: impl Into<String>,
    ) {
        self.files.insert(
            path.to_string(),
            SyncedFile {
                local_hash: local_hash.into(),
                remote_revision: remote_revision.into(),
            },
        );
    }

    /// Forget `path` (it was deleted on both sides).
    pub fn forget(&mut self, path: &str) {
        self.files.remove(path);
    }
}

/// `_sync/state-{provider}.json` — one file per remote so two providers never clobber each other.
fn state_path(book_root: &Path, provider: &str) -> std::path::PathBuf {
    layout::sync_dir(book_root).join(format!("state-{provider}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(layout::sync_dir(dir.path())).unwrap();

        let mut state = SyncState::new("local_folder");
        state.mark_synced("a.md", "hash-a", "rev-a");
        state.save(dir.path()).unwrap();

        let loaded = SyncState::load(dir.path(), "local_folder").unwrap();
        assert_eq!(loaded.get("a.md").unwrap().local_hash, "hash-a");
        assert_eq!(loaded.get("a.md").unwrap().remote_revision, "rev-a");
    }

    #[test]
    fn missing_state_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = SyncState::load(dir.path(), "local_folder").unwrap();
        assert!(loaded.files.is_empty());
        assert_eq!(loaded.provider, "local_folder");
    }
}
