//! [`LocalFolderSync`]: a [`SyncProvider`] backed by a plain directory.
//!
//! This is the realistic default, not a test double: the Google Drive and Dropbox desktop clients
//! both expose the cloud as an ordinary synced folder, so pointing this provider at that folder
//! *is* cloud sync. It is also exactly what the engine tests use — two book directories sharing one
//! "remote" folder stand in for two devices sharing one Drive.
//!
//! The revision token is the content hash (sha256), so a file's revision changes if and only if its
//! bytes change. That is what gives the engine precise, loop-free change detection.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};
use crate::sync::provider::{RemoteEntry, RemoteRevision, SyncProvider, LOCAL_FOLDER_ID};

pub struct LocalFolderSync {
    root: PathBuf,
}

impl LocalFolderSync {
    /// Open (creating if needed) a folder-backed remote rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> CoreResult<LocalFolderSync> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(LocalFolderSync { root })
    }

    fn full_path(&self, rel: &str) -> PathBuf {
        // Paths are book-relative POSIX; rejoin per-segment so they resolve on Windows too.
        let mut path = self.root.clone();
        for segment in rel.split('/').filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }
}

impl SyncProvider for LocalFolderSync {
    fn name(&self) -> &str {
        LOCAL_FOLDER_ID
    }

    fn list(&self) -> CoreResult<Vec<RemoteEntry>> {
        let mut entries = Vec::new();
        collect(&self.root, &self.root, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    fn get(&self, path: &str) -> CoreResult<Vec<u8>> {
        let full = self.full_path(path);
        fs::read(&full).map_err(|e| CoreError::Sync(format!("remote get {path}: {e}")))
    }

    fn put(&self, path: &str, bytes: &[u8]) -> CoreResult<RemoteRevision> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full, bytes).map_err(|e| CoreError::Sync(format!("remote put {path}: {e}")))?;
        Ok(content_revision(bytes))
    }

    fn delete(&self, path: &str) -> CoreResult<()> {
        let full = self.full_path(path);
        match fs::remove_file(&full) {
            Ok(()) => Ok(()),
            // Deleting an already-absent path is a no-op, never an error (idempotent sync).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::Sync(format!("remote delete {path}: {e}"))),
        }
    }
}

/// The content revision of some bytes: their sha256 as lowercase hex. Stable and collision-safe;
/// shared by the provider and the engine's local-change detection so both speak the same language.
pub fn content_revision(bytes: &[u8]) -> RemoteRevision {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Recursively gather files under `dir` as book-relative POSIX entries.
fn collect(dir: &Path, root: &Path, out: &mut Vec<RemoteEntry>) -> CoreResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, root, out)?;
        } else {
            let bytes = fs::read(&path)?;
            let rel = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Sync(format!("relativize remote path: {e}")))?;
            out.push(RemoteEntry {
                path: to_posix(rel),
                revision: content_revision(&bytes),
                size: bytes.len() as u64,
            });
        }
    }
    Ok(())
}

/// Render a relative path with forward slashes regardless of host separator.
fn to_posix(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_list_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let remote = LocalFolderSync::open(dir.path()).unwrap();

        let rev = remote.put("notes/a.md", b"hello").unwrap();
        assert_eq!(rev, content_revision(b"hello"));
        assert_eq!(remote.get("notes/a.md").unwrap(), b"hello");

        let listed = remote.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "notes/a.md");
        assert_eq!(listed[0].revision, rev);

        remote.delete("notes/a.md").unwrap();
        assert!(remote.list().unwrap().is_empty());
        remote.delete("notes/a.md").unwrap(); // idempotent
    }

    #[test]
    fn revision_changes_only_when_content_changes() {
        let dir = tempfile::tempdir().unwrap();
        let remote = LocalFolderSync::open(dir.path()).unwrap();
        let r1 = remote.put("x", b"one").unwrap();
        let r2 = remote.put("x", b"one").unwrap();
        let r3 = remote.put("x", b"two").unwrap();
        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
    }
}
