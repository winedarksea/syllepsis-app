//! Cloud sync index: per-device fragments that let a [`SyncProvider`](super::provider::SyncProvider)
//! learn every remote file's revision **without downloading the file**.
//!
//! The naive `list()` hashes the bytes of every remote file each pass, so a quiet sync still pulls
//! the whole book down. To avoid that, each device writes a small JSON fragment under
//! [`CLOUD_INDEX_DIR`] recording the revision it last wrote for each path. Because every device
//! writes **only its own** `index-{author}-{actor}.json`, there are no overwrite races; readers
//! [`merge`](CloudIndex::merge) all fragments into one view.
//!
//! This is the only non-markdown addition the optimization places on the cloud. It is purely an
//! accelerator: a missing or stale fragment just falls back to the download-and-hash path, so the
//! result is always correct — see [`build_remote_entries`].

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::sync::provider::RemoteEntry;

/// Directory (book-relative) holding the per-device index fragments. The only non-markdown thing the
/// sync optimization writes to the cloud; excluded from the syncable set everywhere.
pub const CLOUD_INDEX_DIR: &str = "_sync_index";

/// Schema version stamped into every fragment so a future format change can be detected.
pub const CLOUD_INDEX_SCHEMA_VERSION: u32 = 1;

/// One device's contribution to the cloud index: the revisions it last published for each path.
/// Stored at `_sync_index/index-{author}-{actor}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudIndexFragment {
    pub schema_version: u32,
    pub book_id: String,
    /// Device id (the existing [`actor_id_for`](super::actor_id_for)).
    pub actor: String,
    /// Who owns this device/fragment; attributes a change to a person, not just a device. Falls back
    /// to `actor` when unset so single-user use needs no setup.
    pub author: String,
    pub updated_at: DateTime<Utc>,
    /// Book-relative path → entry.
    pub entries: BTreeMap<String, IndexEntry>,
}

impl CloudIndexFragment {
    /// Build a fragment for the current instant from already-computed `entries`.
    pub fn new(
        book_id: impl Into<String>,
        actor: impl Into<String>,
        author: impl Into<String>,
        entries: BTreeMap<String, IndexEntry>,
    ) -> CloudIndexFragment {
        CloudIndexFragment {
            schema_version: CLOUD_INDEX_SCHEMA_VERSION,
            book_id: book_id.into(),
            actor: actor.into(),
            author: author.into(),
            updated_at: Utc::now(),
            entries,
        }
    }
}

/// What one device knows about one path: its revision and whether it has been deleted (a tombstone,
/// so deletes propagate even though the file is absent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// `== content_revision(bytes)`, the value `put()` already returns.
    pub revision: String,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
}

/// A merged entry: the winning [`IndexEntry`] plus the author/actor of the fragment it came from, so
/// origin survives the merge with zero per-entry storage cost (author is stored once per fragment).
#[derive(Debug, Clone)]
pub struct MergedEntry {
    pub entry: IndexEntry,
    pub author: String,
    pub actor: String,
}

/// The merged view of every device's fragments: per path, the latest known entry.
#[derive(Debug, Clone, Default)]
pub struct CloudIndex {
    pub entries: BTreeMap<String, MergedEntry>,
}

impl CloudIndex {
    /// Merge fragments into one view. Per path the winner is the entry with the greatest
    /// `updated_at` (ties broken by the `revision` string for determinism), tagged with that
    /// fragment's author/actor. A winning tombstone is kept so callers can treat the path as absent.
    pub fn merge(fragments: impl IntoIterator<Item = CloudIndexFragment>) -> CloudIndex {
        let mut entries: BTreeMap<String, MergedEntry> = BTreeMap::new();
        for fragment in fragments {
            for (path, entry) in fragment.entries {
                let wins = match entries.get(&path) {
                    None => true,
                    Some(existing) => {
                        entry.updated_at > existing.entry.updated_at
                            || (entry.updated_at == existing.entry.updated_at
                                && entry.revision > existing.entry.revision)
                    }
                };
                if wins {
                    entries.insert(
                        path,
                        MergedEntry {
                            entry,
                            author: fragment.author.clone(),
                            actor: fragment.actor.clone(),
                        },
                    );
                }
            }
        }
        CloudIndex { entries }
    }

    /// The winning entry for `path` if present and **not** a tombstone.
    pub fn live(&self, path: &str) -> Option<&MergedEntry> {
        self.entries.get(path).filter(|m| !m.entry.deleted)
    }
}

/// True if `path` is inside the cloud index directory (its fragments must never be treated as user
/// files — excluded from the syncable set and from `list()` output).
pub fn is_cloud_index_path(path: &str) -> bool {
    path.split('/').next().unwrap_or("") == CLOUD_INDEX_DIR
}

/// Book-relative path of one device's fragment. `author` is included so two users' devices never
/// collide and origin is visible at the file level; both parts are sanitized for filesystem safety.
pub fn fragment_path(author: &str, actor: &str) -> String {
    format!(
        "{CLOUD_INDEX_DIR}/index-{}-{}.json",
        sanitize_component(author),
        sanitize_component(actor)
    )
}

/// Reduce an arbitrary author/actor string to a filename-safe token (alphanumerics, `-`, `_`),
/// collapsing everything else to `_`. Keeps human-readable display names usable in the filename.
fn sanitize_component(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

/// One file discovered by a recursive remote listing, before its revision is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRemoteFile {
    pub path: String,
    pub size: u64,
}

/// Build the [`RemoteEntry`] list the engine consumes from a recursive listing and the merged index.
///
/// For each listed file (excluding the index dir): use the index's revision when present and live;
/// otherwise call `fallback` to fetch-and-hash the bytes. The fallback covers pre-index books and
/// files written by external tools, so the result is always correct even when the index is missing
/// or stale — the index only makes the common (quiet) case cheap.
pub fn build_remote_entries<F>(
    listed: impl IntoIterator<Item = ListedRemoteFile>,
    index: &CloudIndex,
    mut fallback: F,
) -> CoreResult<Vec<RemoteEntry>>
where
    F: FnMut(&str) -> CoreResult<String>,
{
    let mut out = Vec::new();
    for file in listed {
        if is_cloud_index_path(&file.path) {
            continue;
        }
        let revision = match index.live(&file.path) {
            Some(merged) => merged.entry.revision.clone(),
            None => fallback(&file.path)?,
        };
        out.push(RemoteEntry {
            path: file.path,
            revision,
            size: file.size,
        });
    }
    Ok(out)
}

/// A fragment-aware test double mirroring [`LocalFolderSync`](super::local_folder::LocalFolderSync)
/// but implementing the cheap `list()` (read fragments instead of hashing every file) and
/// `publish_index`. `get_count` records how many file bodies were fetched (fragments are read out of
/// band and not counted), so tests can assert the bandwidth win.
#[cfg(test)]
pub(crate) struct IndexedLocalFolderSync {
    root: std::path::PathBuf,
    get_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
impl IndexedLocalFolderSync {
    pub(crate) fn open(root: impl Into<std::path::PathBuf>) -> CoreResult<IndexedLocalFolderSync> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(IndexedLocalFolderSync {
            root,
            get_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    /// A shared handle to the content-fetch counter, so a caller that has moved the provider into a
    /// `Box<dyn SyncProvider>` can still observe how many file bodies were fetched.
    pub(crate) fn get_count_handle(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        std::sync::Arc::clone(&self.get_count)
    }

    fn full(&self, rel: &str) -> std::path::PathBuf {
        let mut path = self.root.clone();
        for segment in rel.split('/').filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }
}

#[cfg(test)]
impl super::provider::SyncProvider for IndexedLocalFolderSync {
    fn name(&self) -> &str {
        crate::sync::provider::LOCAL_FOLDER_ID
    }

    fn list(&self) -> CoreResult<Vec<RemoteEntry>> {
        let mut listed = Vec::new();
        let mut fragments = Vec::new();
        let mut paths = Vec::new();
        collect_paths(&self.root, &self.root, &mut paths)?;
        for (path, size) in paths {
            if is_cloud_index_path(&path) {
                if path.ends_with(".json") {
                    // Fragments are read out of band (not counted as a content fetch).
                    if let Ok(bytes) = std::fs::read(self.full(&path)) {
                        if let Ok(fragment) =
                            serde_json::from_slice::<CloudIndexFragment>(&bytes)
                        {
                            fragments.push(fragment);
                        }
                    }
                }
                continue;
            }
            listed.push(ListedRemoteFile { path, size });
        }
        let index = CloudIndex::merge(fragments);
        build_remote_entries(listed, &index, |path| {
            self.get_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(crate::sync::local_folder::content_revision(&std::fs::read(
                self.full(path),
            )?))
        })
    }

    fn get(&self, path: &str) -> CoreResult<Vec<u8>> {
        self.get_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::fs::read(self.full(path))
            .map_err(|e| crate::error::CoreError::Sync(format!("remote get {path}: {e}")))
    }

    fn put(&self, path: &str, bytes: &[u8]) -> CoreResult<crate::sync::provider::RemoteRevision> {
        let full = self.full(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| crate::error::CoreError::Sync(format!("remote put {path}: {e}")))?;
        Ok(crate::sync::local_folder::content_revision(bytes))
    }

    fn delete(&self, path: &str) -> CoreResult<()> {
        match std::fs::remove_file(self.full(path)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(crate::error::CoreError::Sync(format!(
                "remote delete {path}: {e}"
            ))),
        }
    }

    fn publish_index(
        &self,
        actor: &str,
        author: &str,
        book_id: &str,
        entries: &BTreeMap<String, IndexEntry>,
    ) -> CoreResult<()> {
        let fragment = CloudIndexFragment::new(book_id, actor, author, entries.clone());
        let path = fragment_path(author, actor);
        let bytes = serde_json::to_vec_pretty(&fragment)?;
        let full = self.full(&path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| crate::error::CoreError::Sync(format!("publish index {path}: {e}")))
    }
}

#[cfg(test)]
fn collect_paths(
    dir: &std::path::Path,
    root: &std::path::Path,
    out: &mut Vec<(String, u64)>,
) -> CoreResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_paths(&path, root, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let posix = rel
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect::<Vec<_>>()
                .join("/");
            out.push((posix, size));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(revision: &str, secs: i64, deleted: bool) -> IndexEntry {
        IndexEntry {
            revision: revision.to_string(),
            updated_at: DateTime::<Utc>::from_timestamp(secs, 0).unwrap(),
            deleted,
        }
    }

    fn fragment(author: &str, actor: &str, entries: &[(&str, IndexEntry)]) -> CloudIndexFragment {
        CloudIndexFragment::new(
            "book-1",
            actor,
            author,
            entries
                .iter()
                .map(|(p, e)| (p.to_string(), e.clone()))
                .collect(),
        )
    }

    #[test]
    fn merge_keeps_the_latest_entry_per_path() {
        let a = fragment("alice", "actor-a", &[("note.md", entry("rev-old", 10, false))]);
        let b = fragment("bob", "actor-b", &[("note.md", entry("rev-new", 20, false))]);
        let index = CloudIndex::merge([a, b]);
        let merged = index.live("note.md").unwrap();
        assert_eq!(merged.entry.revision, "rev-new");
    }

    #[test]
    fn merge_author_survives() {
        let a = fragment("alice", "actor-a", &[("note.md", entry("rev-old", 10, false))]);
        let b = fragment("bob", "actor-b", &[("note.md", entry("rev-new", 20, false))]);
        let index = CloudIndex::merge([a, b]);
        let merged = index.live("note.md").unwrap();
        assert_eq!(merged.author, "bob");
        assert_eq!(merged.actor, "actor-b");
    }

    #[test]
    fn merge_tombstone_wins_when_later() {
        let live = fragment("alice", "actor-a", &[("note.md", entry("rev", 10, false))]);
        let dead = fragment("bob", "actor-b", &[("note.md", entry("", 20, true))]);
        let index = CloudIndex::merge([live, dead]);
        // Present in the raw map but not "live".
        assert!(index.entries.contains_key("note.md"));
        assert!(index.live("note.md").is_none());
    }

    #[test]
    fn merge_tie_break_is_deterministic_on_revision() {
        let a = fragment("alice", "actor-a", &[("note.md", entry("aaa", 10, false))]);
        let b = fragment("bob", "actor-b", &[("note.md", entry("zzz", 10, false))]);
        // Same timestamp; greater revision string wins, regardless of input order.
        let ab = CloudIndex::merge([a.clone(), b.clone()]);
        let ba = CloudIndex::merge([b, a]);
        assert_eq!(ab.live("note.md").unwrap().entry.revision, "zzz");
        assert_eq!(ba.live("note.md").unwrap().entry.revision, "zzz");
    }

    #[test]
    fn build_remote_entries_uses_index_and_skips_fallback() {
        let index = CloudIndex::merge([fragment(
            "alice",
            "actor-a",
            &[("note.md", entry("indexed-rev", 10, false))],
        )]);
        let listed = vec![ListedRemoteFile {
            path: "note.md".to_string(),
            size: 4,
        }];
        let mut fallbacks = 0;
        let entries = build_remote_entries(listed, &index, |_| {
            fallbacks += 1;
            Ok("hashed".to_string())
        })
        .unwrap();
        assert_eq!(fallbacks, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].revision, "indexed-rev");
    }

    #[test]
    fn build_remote_entries_falls_back_when_unindexed_or_tombstoned() {
        let index = CloudIndex::merge([fragment(
            "alice",
            "actor-a",
            &[("gone.md", entry("", 10, true))],
        )]);
        let listed = vec![
            ListedRemoteFile {
                path: "fresh.md".to_string(),
                size: 1,
            },
            ListedRemoteFile {
                path: "gone.md".to_string(),
                size: 1,
            },
        ];
        let mut fallbacks = 0;
        let entries = build_remote_entries(listed, &index, |_| {
            fallbacks += 1;
            Ok("hashed".to_string())
        })
        .unwrap();
        // Both an unindexed file and a tombstoned (not-live) path hit the fallback.
        assert_eq!(fallbacks, 2);
        assert!(entries.iter().all(|e| e.revision == "hashed"));
    }

    #[test]
    fn build_remote_entries_excludes_index_dir() {
        let index = CloudIndex::default();
        let listed = vec![ListedRemoteFile {
            path: format!("{CLOUD_INDEX_DIR}/index-alice-actor-a.json"),
            size: 9,
        }];
        let entries = build_remote_entries(listed, &index, |_| {
            panic!("index files must not be fetched")
        })
        .unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn fragment_path_is_filesystem_safe() {
        let path = fragment_path("Alice Smith", "actor/42");
        assert_eq!(path, "_sync_index/index-Alice_Smith-actor_42.json");
        assert!(is_cloud_index_path(&path));
    }
}
