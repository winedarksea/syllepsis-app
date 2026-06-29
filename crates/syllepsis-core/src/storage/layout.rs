//! On-disk layout of a book folder (object-types.md "Storage Layout").
//!
//! A book is a directory. Notes are markdown files within it (flat in Phase 1; sorting
//! subfolders are a later enhancement, and because identity lives in frontmatter the path is
//! always disposable). A few underscore-prefixed directories/files are reserved:
//!
//! ```text
//! my-book/
//!   _book.md          book-level metadata (name, language, location, cover, aliases)
//!   _config.yaml      per-book operational config
//!   _categories/      one frontmatter file per category
//!   _commentary/      commentary notes (AI proposals, fact checks)
//!   _worlds/          spatial worlds registry
//!   _crdt/            per-note CRDT sidecars (Phase 4) — synced, not human-edited
//!   _embeddings/      per-note generated embedding sidecars — synced, not human-edited
//!   _sync/            device-local sync bookkeeping (state, actor id) — never synced
//!   _derived/         ephemeral caches (vectors, search index) — gitignored, not synced
//!   note-*.md         notes (and other object types)
//! ```

use std::path::{Path, PathBuf};

use crate::id::NoteId;

pub const CATEGORIES_DIR: &str = "_categories";
pub const COMMENTARY_DIR: &str = "_commentary";
pub const WORLDS_DIR: &str = "_worlds";
pub const DERIVED_DIR: &str = "_derived";
pub const EMBEDDINGS_DIR: &str = "_embeddings";
/// Per-note CRDT sidecars (Phase 4). Synced (so other devices can merge) but excluded from the
/// note scan — it holds `{ulid}.crdt` snapshots, not markdown.
pub const CRDT_DIR: &str = "_crdt";
/// Device-local sync bookkeeping (per-provider state, this device's actor id). Never synced and
/// gitignored: it records *this* machine's view of the remote, which is meaningless elsewhere.
pub const SYNC_DIR: &str = "_sync";
/// Per-pack import manifests (baseline hashes + CRDT snapshots). Device-local: records this
/// machine's imported baseline, not part of the shared book content.
pub const PACKS_DIR: &str = "_packs";
pub const BOOK_META_FILE: &str = "_book.md";
pub const CONFIG_FILE: &str = "_config.yaml";
/// Sidecar extension for the per-note CRDT snapshot files inside `_crdt/`.
pub const CRDT_EXTENSION: &str = "crdt";
/// Text→coordinate lookup table inside `_worlds/` (spatial-worlds.md). A flat CSV (not
/// frontmatter) so it is spreadsheet-editable; lives beside the per-world `{id}.md` files.
pub const LOCATION_LOOKUP_FILE: &str = "locations.csv";

/// Directories that never contain first-class notes and are skipped when scanning for them.
/// Commentary has explicit child-object APIs instead of participating in the normal note corpus.
pub const RESERVED_DIRS: &[&str] = &[
    CATEGORIES_DIR,
    COMMENTARY_DIR,
    WORLDS_DIR,
    DERIVED_DIR,
    EMBEDDINGS_DIR,
    CRDT_DIR,
    SYNC_DIR,
    PACKS_DIR,
];

/// `_categories/` for the given book root.
pub fn categories_dir(root: &Path) -> PathBuf {
    root.join(CATEGORIES_DIR)
}

/// `_commentary/` for the given book root.
pub fn commentary_dir(root: &Path) -> PathBuf {
    root.join(COMMENTARY_DIR)
}

/// `_worlds/` for the given book root.
pub fn worlds_dir(root: &Path) -> PathBuf {
    root.join(WORLDS_DIR)
}

/// The text→coordinate lookup CSV path (`_worlds/locations.csv`).
pub fn location_lookup_path(root: &Path) -> PathBuf {
    worlds_dir(root).join(LOCATION_LOOKUP_FILE)
}

/// `_derived/` for the given book root (ephemeral, gitignored).
pub fn derived_dir(root: &Path) -> PathBuf {
    root.join(DERIVED_DIR)
}

/// `_embeddings/` for synced per-note embedding records.
pub fn embeddings_dir(root: &Path) -> PathBuf {
    root.join(EMBEDDINGS_DIR)
}

/// Binary embedding sidecar path, keyed by immutable note ULID.
pub fn embedding_sidecar_path(root: &Path, note: &NoteId) -> PathBuf {
    embeddings_dir(root).join(format!("{}.svec", note.ulid()))
}

/// `_crdt/` for the given book root (per-note CRDT sidecars, synced).
pub fn crdt_dir(root: &Path) -> PathBuf {
    root.join(CRDT_DIR)
}

/// `_sync/` for the given book root (device-local sync bookkeeping).
pub fn sync_dir(root: &Path) -> PathBuf {
    root.join(SYNC_DIR)
}

/// `_packs/` for the given book root (per-pack import manifests, device-local).
pub fn packs_dir(root: &Path) -> PathBuf {
    root.join(PACKS_DIR)
}

/// The manifest file for a specific pack: `_packs/{pack_id}.json`.
pub fn pack_manifest_path(root: &Path, pack_id: &str) -> PathBuf {
    packs_dir(root).join(format!("{pack_id}.json"))
}

/// The CRDT sidecar path for a note: `_crdt/{ulid}.crdt`. Keyed on the ulid (not the slug-bearing
/// filename) so a retitle/move of the note never orphans its sidecar — identity is the ulid.
pub fn crdt_sidecar_path(root: &Path, note: &NoteId) -> PathBuf {
    crdt_dir(root).join(format!("{}.{CRDT_EXTENSION}", note.ulid()))
}

/// The book-level metadata file path.
pub fn book_meta_path(root: &Path) -> PathBuf {
    root.join(BOOK_META_FILE)
}

/// The per-book config file path.
pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_FILE)
}

/// Default filename for a note: `{id}.md`. The id is filename-safe on every platform (no
/// colons), so this is a clean default even though the canonical id lives in frontmatter.
pub fn note_filename(id: &NoteId) -> String {
    format!("{}.md", id.as_str())
}

/// CSV companion path for a Table note: `{id}.csv`, lives beside the `.md` file.
pub fn table_companion_csv_path(root: &Path, id: &NoteId) -> PathBuf {
    root.join(format!("{}.csv", id.as_str()))
}

/// Filename for a category file inside `_categories/`.
pub fn category_filename(name: &str) -> String {
    format!("{name}.md")
}

/// Filename for a world registry file inside `_worlds/` (`{id}.md`). World ids are slugs without
/// path-hostile characters, so they are filename-safe like note ids.
pub fn world_filename(id: &str) -> String {
    format!("{id}.md")
}

/// True if a directory name is reserved (should be skipped by the note scan).
pub fn is_reserved_dir(name: &str) -> bool {
    RESERVED_DIRS.contains(&name)
}

/// True if a top-level file is a reserved book file (book meta / config), not a note.
pub fn is_reserved_file(name: &str) -> bool {
    name == BOOK_META_FILE || name == CONFIG_FILE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_filename_uses_full_id() {
        let id = NoteId::generate("note", "hello world");
        let filename = note_filename(&id);
        assert!(filename.ends_with(".md"));
        assert!(filename.starts_with(id.as_str()));
        assert!(!filename.contains(':'));
    }

    #[test]
    fn reserved_dirs_include_commentary() {
        assert!(is_reserved_dir(CATEGORIES_DIR));
        assert!(is_reserved_dir(DERIVED_DIR));
        assert!(is_reserved_dir(COMMENTARY_DIR));
    }
}
