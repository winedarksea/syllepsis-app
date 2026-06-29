//! Per-book pack import manifests: the authoritative record of which notes belong to a pack and
//! what baseline content was imported (hash + CRDT snapshot) so re-import can detect local edits
//! without relying on an eager flag.
//!
//! Each pack gets its own file at `_packs/{pack_id}.json` (device-local, never synced — mirrors
//! the pattern of `_sync/` state files).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::storage::layout;

/// Per-note baseline recorded at import time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBaseline {
    /// The pack version at which this baseline was captured.
    pub pack_version: String,
    /// SHA-256 hex of the imported body — used for modification detection.
    pub base_body_sha256: String,
    /// Which CRDT backend was active when the baseline was captured (`lww` or `loro`).
    pub crdt_backend: String,
    /// Base-64 CRDT snapshot seeded from the imported body. Present when Loro was active; used
    /// for 3-way merge. May be absent for LWW-captured baselines (empty string = absent).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_crdt_snapshot_b64: String,
}

/// The manifest for one pack inside a book: membership list + per-note baselines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BookPackManifest {
    pub pack_id: String,
    pub version: String,
    pub imported_at: String,
    /// `note_id (string) → NoteBaseline`
    pub notes: BTreeMap<String, NoteBaseline>,
}

impl BookPackManifest {
    pub fn new(pack_id: impl Into<String>, version: impl Into<String>) -> Self {
        BookPackManifest {
            pack_id: pack_id.into(),
            version: version.into(),
            imported_at: chrono::Utc::now().to_rfc3339(),
            notes: BTreeMap::new(),
        }
    }

    /// Load the manifest for `pack_id` from `book_root/_packs/{pack_id}.json`. Returns a fresh
    /// default manifest when the file does not exist yet.
    pub fn load(book_root: &Path, pack_id: &str) -> CoreResult<Self> {
        let path = layout::pack_manifest_path(book_root, pack_id);
        if !path.exists() {
            return Ok(BookPackManifest {
                pack_id: pack_id.to_string(),
                ..Default::default()
            });
        }
        let json = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Persist the manifest to `book_root/_packs/{pack_id}.json`, creating the `_packs/`
    /// directory if it doesn't exist yet.
    pub fn save(&self, book_root: &Path) -> CoreResult<()> {
        let dir = layout::packs_dir(book_root);
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        let path = layout::pack_manifest_path(book_root, &self.pack_id);
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
