//! The [`SyncProvider`] seam: a user-owned remote store the book is pushed to and pulled from.
//!
//! Syllepsis hosts nothing itself — the user connects their own cloud (Google Drive, GitHub; see
//! sync-backup.md). Every such target is reduced to four operations over book-relative paths, so
//! the [`SyncEngine`](super::engine::SyncEngine) never knows which cloud it is talking to. The
//! built-in [`LocalFolderSync`](super::local_folder::LocalFolderSync) implements this against a
//! plain directory, which is exactly how the Google Drive / Dropbox desktop apps expose a synced
//! folder — so it is a genuinely useful default, not a stub, and the one the tests run against.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;

/// An opaque per-file revision token (an etag, a Drive version id, a git blob sha, or — for the
/// local-folder provider — a content hash). The engine compares these for *equality only*; it
/// never parses one. A changed file yields a different revision; an unchanged file the same one.
pub type RemoteRevision = String;

/// One file present on the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// Book-relative POSIX path (forward slashes on every platform).
    pub path: String,
    /// Revision of the bytes currently at `path`.
    pub revision: RemoteRevision,
    /// Size in bytes, for progress reporting.
    pub size: u64,
}

/// A user-owned remote the book syncs to. Implementations return [`CoreError::Sync`] on I/O
/// failure rather than panicking, so a flaky network never corrupts the local book.
pub trait SyncProvider: Send {
    /// Short identifier for diagnostics and sync state (`local_folder`, `google_drive`, `github`).
    fn name(&self) -> &str;

    /// Every file currently on the remote, with its revision.
    fn list(&self) -> CoreResult<Vec<RemoteEntry>>;

    /// Fetch the bytes at `path`.
    fn get(&self, path: &str) -> CoreResult<Vec<u8>>;

    /// Write `bytes` to `path` (creating it if absent), returning the new revision.
    fn put(&self, path: &str, bytes: &[u8]) -> CoreResult<RemoteRevision>;

    /// Remove `path` from the remote. Removing an already-absent path is not an error.
    fn delete(&self, path: &str) -> CoreResult<()>;
}

/// How a remote stores data, which determines how the app treats it (sync-backup.md): a *drive*
/// gets the full backup and is the primary CRDT target; *git* is the lower-priority "public,
/// partial rolling release" carrying only human-readable markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncProviderKind {
    /// A synced folder (local filesystem, or a cloud drive's desktop mount).
    Drive,
    /// A git remote (commits as deliberate releases, not every keystroke).
    Git,
}

/// UI-facing description of a sync target the app knows how to offer. Pure data (no I/O), so the
/// settings screen can list targets and label which are wired in this build honestly via
/// [`implemented`](SyncProviderDescriptor::implemented).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncProviderDescriptor {
    /// Stable id used in [`SyncConfig`](crate::config::SyncConfig) and sync state.
    pub id: String,
    pub display_name: String,
    pub kind: SyncProviderKind,
    /// Whether connecting needs OAuth / a token (vs. a local path).
    pub requires_auth: bool,
    /// Whether a working `impl SyncProvider` exists in this build. Only the local folder provider
    /// is wired into the core today; the cloud HTTP providers are declared here for the UI roadmap
    /// but not yet implemented (kept honest per AGENTS.md — no pretend fallbacks).
    pub implemented: bool,
}

/// Stable id of the built-in local-folder sync provider.
pub const LOCAL_FOLDER_ID: &str = "local_folder";
/// Stable id of the (planned) Google Drive provider.
pub const GOOGLE_DRIVE_ID: &str = "google_drive";
/// Stable id of the (planned) GitHub provider.
pub const GITHUB_ID: &str = "github";

/// Every sync target the app advertises. Downloaded/authorized lazily; presence here does not imply
/// a connection is configured.
pub fn provider_descriptors() -> Vec<SyncProviderDescriptor> {
    vec![
        SyncProviderDescriptor {
            id: LOCAL_FOLDER_ID.to_string(),
            display_name: "Local / mounted folder".to_string(),
            kind: SyncProviderKind::Drive,
            requires_auth: false,
            implemented: true,
        },
        SyncProviderDescriptor {
            id: GOOGLE_DRIVE_ID.to_string(),
            display_name: "Google Drive".to_string(),
            kind: SyncProviderKind::Drive,
            requires_auth: true,
            implemented: false,
        },
        SyncProviderDescriptor {
            id: GITHUB_ID.to_string(),
            display_name: "GitHub".to_string(),
            kind: SyncProviderKind::Git,
            requires_auth: true,
            implemented: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_folder_is_the_only_implemented_provider_today() {
        let descriptors = provider_descriptors();
        let implemented: Vec<&str> = descriptors
            .iter()
            .filter(|d| d.implemented)
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(implemented, vec![LOCAL_FOLDER_ID]);
        // Cloud providers are advertised for the UI but honestly flagged unimplemented.
        assert!(descriptors
            .iter()
            .any(|d| d.id == GOOGLE_DRIVE_ID && !d.implemented));
        assert!(descriptors
            .iter()
            .any(|d| d.id == GITHUB_ID && d.requires_auth));
    }
}
