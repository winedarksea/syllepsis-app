//! UUID sidecars for binary assets (images, imported SVG drawings).
//!
//! A CRDT cannot meaningfully merge binary blobs, so images are *not* tracked in the live document
//! (sync-backup.md). Instead each asset gets a tiny `{asset}.uuid` sidecar holding a stable id.
//! The app references assets by that id, so moving or renaming the file — which changes its path —
//! never breaks a note's reference: the [`AssetRegistry`] re-points the id to wherever the file
//! now lives on the next scan. (Overlay anchors that link notes to drawing coordinates live in note
//! metadata, which *is* CRDT-tracked; only the heavy geometry is file-synced.)

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::CoreResult;

/// Extension of an asset's id sidecar file.
const UUID_EXTENSION: &str = "uuid";

/// File extensions treated as binary assets that get UUID sidecars. SVG is included even though it
/// is text: arbitrary imported SVG geometry is file-synced, not CRDT'd (sync-backup.md "Drawings
/// and SVG").
const ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];

/// One asset's stable identity and its current location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSidecar {
    pub uuid: String,
    /// Book-relative POSIX path of the asset the sidecar belongs to.
    pub asset_path: String,
}

/// A book's assets indexed by their stable UUIDs, rebuilt by scanning `.uuid` sidecars. Rebuildable
/// from disk, so it is never itself synced — the `.uuid` sidecar files are the synced source.
#[derive(Debug, Clone, Default)]
pub struct AssetRegistry {
    by_uuid: BTreeMap<String, String>,
}

impl AssetRegistry {
    /// Scan a book for asset files and read each one's `.uuid` sidecar, mapping id → current path.
    /// Assets without a sidecar are skipped (untracked until [`assign`](AssetRegistry::assign)).
    pub fn scan(book_root: &Path) -> CoreResult<AssetRegistry> {
        let mut by_uuid = BTreeMap::new();
        let mut files = Vec::new();
        collect(book_root, book_root, &mut files)?;
        for rel in files {
            if !is_asset(&rel) {
                continue;
            }
            if let Some(uuid) = read_sidecar(book_root, &rel)? {
                by_uuid.insert(uuid, rel);
            }
        }
        Ok(AssetRegistry { by_uuid })
    }

    /// The current path of the asset with `uuid`, if tracked.
    pub fn resolve(&self, uuid: &str) -> Option<&str> {
        self.by_uuid.get(uuid).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_uuid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_uuid.is_empty()
    }

    /// Iterate all tracked assets as `(uuid, book-relative-path)` pairs.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.by_uuid
            .iter()
            .map(|(uuid, path)| (uuid.as_str(), path.as_str()))
    }
}

/// Ensure `asset_rel` has a UUID sidecar, returning its id. Idempotent: an existing sidecar's id is
/// reused (so re-running never re-ids a tracked asset), otherwise a fresh ulid is minted and the
/// `{asset}.uuid` sidecar written next to the file.
pub fn assign(book_root: &Path, asset_rel: &str) -> CoreResult<String> {
    if let Some(existing) = read_sidecar(book_root, asset_rel)? {
        return Ok(existing);
    }
    let uuid = ulid::Ulid::new().to_string().to_lowercase();
    let sidecar = sidecar_full_path(book_root, asset_rel);
    if let Some(parent) = sidecar.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&sidecar, &uuid)?;
    Ok(uuid)
}

/// Read the UUID recorded for `asset_rel`, if its sidecar exists.
fn read_sidecar(book_root: &Path, asset_rel: &str) -> CoreResult<Option<String>> {
    let sidecar = sidecar_full_path(book_root, asset_rel);
    match std::fs::read_to_string(&sidecar) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn sidecar_full_path(book_root: &Path, asset_rel: &str) -> std::path::PathBuf {
    let mut path = book_root.to_path_buf();
    for segment in asset_rel.split('/').filter(|s| !s.is_empty()) {
        path.push(segment);
    }
    path.set_file_name(format!(
        "{}.{UUID_EXTENSION}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("asset")
    ));
    path
}

/// True if a book-relative path has an asset extension.
fn is_asset(rel: &str) -> bool {
    rel.rsplit('.')
        .next()
        .map(|ext| ASSET_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Recursively collect book-relative POSIX file paths.
fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) -> CoreResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, root, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(
                rel.components()
                    .filter_map(|c| c.as_os_str().to_str())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_is_idempotent_and_scannable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cover.png"), b"\x89PNG fake").unwrap();

        let uuid = assign(dir.path(), "cover.png").unwrap();
        // Re-assigning returns the same id (the sidecar is reused, not regenerated).
        assert_eq!(assign(dir.path(), "cover.png").unwrap(), uuid);

        let registry = AssetRegistry::scan(dir.path()).unwrap();
        assert_eq!(registry.resolve(&uuid), Some("cover.png"));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn move_is_tracked_by_uuid_not_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jpg"), b"jpeg").unwrap();
        let uuid = assign(dir.path(), "a.jpg").unwrap();

        // Simulate a move: relocate both the asset and its sidecar into a subfolder.
        std::fs::create_dir_all(dir.path().join("img")).unwrap();
        std::fs::rename(dir.path().join("a.jpg"), dir.path().join("img/b.jpg")).unwrap();
        std::fs::rename(
            dir.path().join("a.jpg.uuid"),
            dir.path().join("img/b.jpg.uuid"),
        )
        .unwrap();

        let registry = AssetRegistry::scan(dir.path()).unwrap();
        // Same id, new path — the note's reference by uuid still resolves.
        assert_eq!(registry.resolve(&uuid), Some("img/b.jpg"));
    }

    #[test]
    fn non_assets_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), b"# text").unwrap();
        let registry = AssetRegistry::scan(dir.path()).unwrap();
        assert!(registry.is_empty());
    }
}
