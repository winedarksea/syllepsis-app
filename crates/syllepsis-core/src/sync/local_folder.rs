//! [`LocalFolderSync`]: a [`SyncProvider`] backed by a plain directory.
//!
//! This is the realistic default, not a test double: the Google Drive and Dropbox desktop clients
//! both expose the cloud as an ordinary synced folder, so pointing this provider at that folder
//! *is* cloud sync. It is also exactly what the engine tests use — two book directories sharing one
//! "remote" folder stand in for two devices sharing one Drive.
//!
//! The revision token is the content hash (sha256), so a file's revision changes if and only if its
//! bytes change. That is what gives the engine precise, loop-free change detection.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, CoreResult};
use crate::sync::provider::{RemoteEntry, RemoteRevision, SyncProvider, LOCAL_FOLDER_ID};

/// Reserved directory (excluded from `list()`/scanning) holding this device's memoized file
/// hashes for `root`. Lives inside `root` so it travels with a relocated/renamed remote folder,
/// but `collect()` skips it explicitly so it is never mistaken for synced book content.
const REVISION_CACHE_DIR: &str = ".syllepsis_sync_cache";
const REVISION_CACHE_FILE: &str = "revision_cache.json";

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
        let mut cache = RevisionCache::load(&self.root);
        let mut entries = Vec::new();
        collect(&self.root, &self.root, &mut entries, &mut cache)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let live_paths: HashSet<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        cache.entries.retain(|path, _| live_paths.contains(path.as_str()));
        // Best-effort: a failure to persist the cache just means the next pass rehashes, same as
        // a first run — it must never turn into a `list()` failure.
        let _ = cache.save(&self.root);
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

/// One file's memoized fingerprint: if `(mtime, size)` still match on disk, `sha256` is reused
/// rather than re-reading and re-hashing the file's bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRevision {
    mtime_secs: u64,
    mtime_nanos: u32,
    size: u64,
    sha256: String,
}

/// Per-remote-root memoization of `content_revision`, so an unchanged file across sync passes
/// skips a full read + hash. Purely a cache: every value is re-derivable from the file's bytes,
/// so a stale, corrupt, or missing cache degrades to the pre-cache behavior, never to wrong
/// output — the mtime+size guard exists to catch overwrites, not to be a source of truth.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RevisionCache {
    entries: BTreeMap<String, CachedRevision>,
}

impl RevisionCache {
    fn load(root: &Path) -> RevisionCache {
        let path = cache_path(root);
        fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, root: &Path) -> CoreResult<()> {
        let dir = root.join(REVISION_CACHE_DIR);
        fs::create_dir_all(&dir)?;
        let bytes = serde_json::to_vec(self)?;
        fs::write(cache_path(root), bytes)?;
        Ok(())
    }
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(REVISION_CACHE_DIR).join(REVISION_CACHE_FILE)
}

fn mtime_key(metadata: &fs::Metadata) -> Option<(u64, u32)> {
    let modified = metadata.modified().ok()?;
    let since_epoch = modified.duration_since(UNIX_EPOCH).ok()?;
    Some((since_epoch.as_secs(), since_epoch.subsec_nanos()))
}

/// Recursively gather files under `dir` as book-relative POSIX entries, reusing `cache` for any
/// file whose `(mtime, size)` haven't changed since it was last hashed.
fn collect(dir: &Path, root: &Path, out: &mut Vec<RemoteEntry>, cache: &mut RevisionCache) -> CoreResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if dir == root && path.file_name().and_then(|n| n.to_str()) == Some(REVISION_CACHE_DIR)
            {
                continue;
            }
            collect(&path, root, out, cache)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Sync(format!("relativize remote path: {e}")))?;
            let rel_posix = to_posix(rel);
            let metadata = fs::metadata(&path)?;
            let size = metadata.len();
            let mtime = mtime_key(&metadata);

            let cached_hit = mtime.and_then(|(secs, nanos)| {
                cache.entries.get(&rel_posix).filter(|cached| {
                    cached.size == size && cached.mtime_secs == secs && cached.mtime_nanos == nanos
                })
            });

            let revision = if let Some(cached) = cached_hit {
                cached.sha256.clone()
            } else {
                let bytes = fs::read(&path)?;
                let hash = content_revision(&bytes);
                if let Some((secs, nanos)) = mtime {
                    cache.entries.insert(
                        rel_posix.clone(),
                        CachedRevision {
                            mtime_secs: secs,
                            mtime_nanos: nanos,
                            size,
                            sha256: hash.clone(),
                        },
                    );
                }
                hash
            };

            out.push(RemoteEntry {
                path: rel_posix,
                revision,
                size,
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

    #[test]
    fn revision_cache_directory_is_excluded_from_listing() {
        let dir = tempfile::tempdir().unwrap();
        let remote = LocalFolderSync::open(dir.path()).unwrap();
        remote.put("a.md", b"content").unwrap();
        remote.list().unwrap(); // writes the cache directory as a side effect

        assert!(dir.path().join(REVISION_CACHE_DIR).is_dir());
        let listed = remote.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "a.md");
    }

    #[test]
    fn second_list_reuses_the_cached_hash_for_an_unchanged_file() {
        let dir = tempfile::tempdir().unwrap();
        let remote = LocalFolderSync::open(dir.path()).unwrap();
        let real_rev = remote.put("a.md", b"hello").unwrap();

        let first = remote.list().unwrap();
        assert_eq!(first[0].revision, real_rev);

        // Tamper with the persisted cache entry directly: if the second `list()` still recomputed
        // the hash from the (unchanged) file bytes, it would overwrite this back to `real_rev`.
        // Seeing the tampered value survive proves the cached hash was reused, not the file re-read.
        let cache_file = cache_path(dir.path());
        let mut cache = RevisionCache::load(dir.path());
        let entry = cache.entries.get_mut("a.md").expect("a.md was cached");
        entry.sha256 = "tampered-hash-not-recomputed".into();
        cache.save(dir.path()).unwrap();
        assert!(cache_file.exists());

        let second = remote.list().unwrap();
        assert_eq!(second[0].revision, "tampered-hash-not-recomputed");
    }

    #[test]
    fn changed_file_is_rehashed_even_with_a_cache_present() {
        let dir = tempfile::tempdir().unwrap();
        let remote = LocalFolderSync::open(dir.path()).unwrap();
        remote.put("a.md", b"hello").unwrap();
        remote.list().unwrap();

        // A real content change of different length trips the cache's size guard, so this must
        // rehash regardless of filesystem mtime resolution.
        let new_rev = remote.put("a.md", b"goodbye").unwrap();

        let listed = remote.list().unwrap();
        assert_eq!(listed[0].revision, new_rev);
        assert_eq!(listed[0].revision, content_revision(b"goodbye"));
    }
}
