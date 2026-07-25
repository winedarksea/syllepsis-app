//! The note/category persistence seam.
//!
//! [`NoteStore`] is the trait the rest of the crate depends on; [`FsNoteStore`] is the
//! native-filesystem implementation. A future PWA build supplies an OPFS implementation of
//! the same trait without the rest of the app caring (platform-infra.md).
//!
//! Lookups resolve on the **ulid**, never the filename: `FsNoteStore` keeps a `ulid → path`
//! index built by reading each file's frontmatter id. This is what lets a note be renamed or
//! moved externally (changing its filename) without losing identity — the canonical id lives
//! in frontmatter, and the index re-points to wherever the file now is on the next refresh.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::frontmatter::{self, split_frontmatter};
use crate::model::{Category, Note, World};
use crate::spatial::LocationLookup;
use crate::storage::atomic::write_atomic;
use crate::storage::layout;

/// A note file that could not be loaded during a scan (bad frontmatter, unparseable id, etc).
/// The underlying file is never touched — this is reporting only, so a corrupt file can never be
/// mistaken for a deleted one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteLoadIssue {
    /// Book-relative path (forward-slash separated) of the offending file.
    pub path: String,
    /// The note's id, if the frontmatter id field itself could be parsed.
    pub ulid: Option<String>,
    /// Human-readable description of what went wrong.
    pub error: String,
}

/// Persistence operations for one book's notes and categories.
pub trait NoteStore {
    fn note_ids(&self) -> CoreResult<Vec<NoteId>>;
    fn read_note(&self, id: &NoteId) -> CoreResult<Note>;
    fn write_note(&self, note: &Note) -> CoreResult<()>;
    fn delete_note(&self, id: &NoteId) -> CoreResult<()>;
    fn read_all_notes(&self) -> CoreResult<Vec<Note>>;

    fn categories(&self) -> CoreResult<Vec<Category>>;
    fn read_category(&self, name: &str) -> CoreResult<Category>;
    fn write_category(&self, category: &Category) -> CoreResult<()>;
    fn delete_category(&self, name: &str) -> CoreResult<()>;

    /// User-defined worlds stored under `_worlds/` (the implicit `earth` world is *not* persisted;
    /// the [`WorldRegistry`](crate::spatial::WorldRegistry) adds it).
    fn worlds(&self) -> CoreResult<Vec<World>>;
    fn write_world(&self, world: &World) -> CoreResult<()>;
    fn delete_world(&self, id: &str) -> CoreResult<()>;

    /// The text→coordinate lookup table (`_worlds/locations.csv`); an empty table when absent.
    fn read_location_lookup(&self) -> CoreResult<LocationLookup>;
    fn write_location_lookup(&self, lookup: &LocationLookup) -> CoreResult<()>;
}

/// Minimal frontmatter shape for the index scan — tolerates schema drift in other fields so a
/// single newer/odd note can never lock the whole book out of being indexed.
#[derive(Deserialize)]
struct IdOnly {
    id: NoteId,
}

pub struct FsNoteStore {
    root: PathBuf,
    /// ulid → absolute path of the note file. Interior-mutable so the trait can take `&self`.
    index: RwLock<HashMap<String, PathBuf>>,
    /// Files whose frontmatter id could not be read on the last `refresh()` (so they never made
    /// it into `index`). Reporting only: nothing in this module ever deletes, moves, or de-indexes
    /// a file because it appears here.
    id_scan_issues: RwLock<Vec<NoteLoadIssue>>,
    /// Indexed files whose full body failed to parse on the last `read_all_notes()`.
    parse_issues: RwLock<Vec<NoteLoadIssue>>,
}

impl FsNoteStore {
    /// Open a store rooted at `root`, building the id index from the files already present.
    pub fn open(root: impl Into<PathBuf>) -> CoreResult<FsNoteStore> {
        let store = FsNoteStore {
            root: root.into(),
            index: RwLock::new(HashMap::new()),
            id_scan_issues: RwLock::new(Vec::new()),
            parse_issues: RwLock::new(Vec::new()),
        };
        store.refresh()?;
        Ok(store)
    }

    /// Rebuild the ulid → path index by scanning the book directory. Call after an external
    /// sync pulls in changes. Files whose frontmatter id cannot be parsed are recorded as a
    /// [`NoteLoadIssue`] (surfaced via [`load_issues`](Self::load_issues)) rather than failing
    /// the whole scan.
    pub fn refresh(&self) -> CoreResult<()> {
        let mut files = Vec::new();
        collect_note_files(&self.root, &self.root, &mut files)?;

        let mut index = HashMap::new();
        let mut issues = Vec::new();
        for path in files {
            match read_frontmatter_id(&path) {
                Ok(Some(id)) => {
                    index.insert(id.ulid().to_string(), path);
                }
                Ok(None) => issues.push(NoteLoadIssue {
                    path: self.relative_path(&path),
                    ulid: None,
                    error: "missing frontmatter".to_string(),
                }),
                Err(error) => issues.push(NoteLoadIssue {
                    path: self.relative_path(&path),
                    ulid: None,
                    error: error.to_string(),
                }),
            }
        }
        *self.index.write().expect("index lock poisoned") = index;
        *self
            .id_scan_issues
            .write()
            .expect("id_scan_issues lock poisoned") = issues;
        Ok(())
    }

    /// Files that could not be loaded on the last scan (bad frontmatter, unparseable id or body,
    /// etc). The underlying files are left untouched on disk.
    pub fn load_issues(&self) -> Vec<NoteLoadIssue> {
        let mut issues = self
            .id_scan_issues
            .read()
            .expect("id_scan_issues lock poisoned")
            .clone();
        issues.extend(
            self.parse_issues
                .read()
                .expect("parse_issues lock poisoned")
                .iter()
                .cloned(),
        );
        issues
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|rel| {
                rel.components()
                    .filter_map(|c| c.as_os_str().to_str())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_else(|_| path.display().to_string())
    }

    fn path_for(&self, id: &NoteId) -> Option<PathBuf> {
        self.index
            .read()
            .expect("index lock poisoned")
            .get(id.ulid())
            .cloned()
    }
}

impl NoteStore for FsNoteStore {
    fn note_ids(&self) -> CoreResult<Vec<NoteId>> {
        self.read_all_notes()
            .map(|notes| notes.into_iter().map(|n| n.id).collect())
    }

    fn read_note(&self, id: &NoteId) -> CoreResult<Note> {
        let path = self
            .path_for(id)
            .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.refresh()?;
                let path = self
                    .path_for(id)
                    .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
                fs::read_to_string(&path).map_err(|error| {
                    if error.kind() == ErrorKind::NotFound {
                        CoreError::NotFound(id.to_string())
                    } else {
                        error.into()
                    }
                })?
            }
            Err(error) => return Err(error.into()),
        };
        frontmatter::parse_note(&content)
    }

    fn write_note(&self, note: &Note) -> CoreResult<()> {
        let ulid = note.id.ulid().to_string();
        let existing = self.path_for(&note.id);
        // Preserve the note's directory (e.g. a sorting subfolder); only the filename tracks
        // the current slug.
        let dir = existing
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone());
        fs::create_dir_all(&dir)?;
        let target = dir.join(layout::note_filename(&note.id));

        write_atomic(&target, frontmatter::serialize_note(note)?.as_bytes())?;
        // If the slug changed, the old filename is stale — remove it so there is one file.
        if let Some(old) = existing {
            if old != target {
                let _ = fs::remove_file(old);
            }
        }
        self.index
            .write()
            .expect("index lock poisoned")
            .insert(ulid, target);
        Ok(())
    }

    fn delete_note(&self, id: &NoteId) -> CoreResult<()> {
        let path = self
            .path_for(id)
            .ok_or_else(|| CoreError::NotFound(id.to_string()))?;
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.index
            .write()
            .expect("index lock poisoned")
            .remove(id.ulid());
        Ok(())
    }

    fn read_all_notes(&self) -> CoreResult<Vec<Note>> {
        let indexed_paths: Vec<(String, PathBuf)> = self
            .index
            .read()
            .expect("index lock poisoned")
            .iter()
            .map(|(ulid, path)| (ulid.clone(), path.clone()))
            .collect();
        let mut notes = Vec::with_capacity(indexed_paths.len());
        let mut missing_ulids = Vec::new();
        let mut parse_issues = Vec::new();
        for (ulid, path) in indexed_paths {
            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    missing_ulids.push(ulid);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            // A corrupt note (bad frontmatter, bad enum/date field, etc) must never take the
            // whole book down: report it and leave the file untouched, don't propagate `?`.
            match frontmatter::parse_note(&content) {
                Ok(note) => notes.push(note),
                Err(error) => parse_issues.push(NoteLoadIssue {
                    path: self.relative_path(&path),
                    ulid: Some(ulid),
                    error: error.to_string(),
                }),
            }
        }
        if !missing_ulids.is_empty() {
            let mut index = self.index.write().expect("index lock poisoned");
            for ulid in missing_ulids {
                index.remove(&ulid);
            }
        }
        *self
            .parse_issues
            .write()
            .expect("parse_issues lock poisoned") = parse_issues;
        Ok(notes)
    }

    fn categories(&self) -> CoreResult<Vec<Category>> {
        let dir = layout::categories_dir(&self.root);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut categories = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = fs::read_to_string(&path)?;
                categories.push(parse_category(&content)?);
            }
        }
        Ok(categories)
    }

    fn read_category(&self, name: &str) -> CoreResult<Category> {
        // Validated on the read path too, not just the write path: a hostile name reaching a read
        // would otherwise let a caller slurp arbitrary files from outside the book.
        let path = layout::categories_dir(&self.root).join(layout::category_filename(name)?);
        let content = fs::read_to_string(&path).map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                CoreError::NotFound(name.to_string())
            } else {
                error.into()
            }
        })?;
        parse_category(&content)
    }

    fn write_category(&self, category: &Category) -> CoreResult<()> {
        // Validate before `create_dir_all` so a hostile name cannot even create directories.
        let filename = layout::category_filename(&category.name)?;
        let dir = layout::categories_dir(&self.root);
        fs::create_dir_all(&dir)?;
        let path = dir.join(filename);
        write_atomic(&path, serialize_category(category)?.as_bytes())?;
        Ok(())
    }

    fn delete_category(&self, name: &str) -> CoreResult<()> {
        let path = layout::categories_dir(&self.root).join(layout::category_filename(name)?);
        fs::remove_file(&path)?;
        Ok(())
    }

    fn worlds(&self) -> CoreResult<Vec<World>> {
        let dir = layout::worlds_dir(&self.root);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut worlds = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            // Skip the lookup CSV (and any non-markdown) — only `{id}.md` files are worlds.
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = fs::read_to_string(&path)?;
                worlds.push(parse_world(&content)?);
            }
        }
        Ok(worlds)
    }

    fn write_world(&self, world: &World) -> CoreResult<()> {
        let filename = layout::world_filename(&world.id)?;
        let dir = layout::worlds_dir(&self.root);
        fs::create_dir_all(&dir)?;
        let path = dir.join(filename);
        write_atomic(&path, serialize_world(world)?.as_bytes())?;
        Ok(())
    }

    fn delete_world(&self, id: &str) -> CoreResult<()> {
        let path = layout::worlds_dir(&self.root).join(layout::world_filename(id)?);
        fs::remove_file(&path)?;
        Ok(())
    }

    fn read_location_lookup(&self) -> CoreResult<LocationLookup> {
        let path = layout::location_lookup_path(&self.root);
        if !path.exists() {
            return Ok(LocationLookup::new());
        }
        LocationLookup::from_csv(&fs::read_to_string(&path)?)
    }

    fn write_location_lookup(&self, lookup: &LocationLookup) -> CoreResult<()> {
        let dir = layout::worlds_dir(&self.root);
        fs::create_dir_all(&dir)?;
        write_atomic(
            &layout::location_lookup_path(&self.root),
            lookup.to_csv().as_bytes(),
        )?;
        Ok(())
    }
}

/// Read only the frontmatter id of a file (resilient to other fields drifting). `Ok(None)` means
/// the file has no frontmatter block at all; a malformed id (bad YAML or bad id string) is an
/// `Err` so the caller can report *why* the scan skipped the file.
fn read_frontmatter_id(path: &Path) -> CoreResult<Option<NoteId>> {
    let content = fs::read_to_string(path)?;
    let Some((frontmatter, _body)) = split_frontmatter(&content) else {
        return Ok(None);
    };
    match serde_yaml::from_str::<IdOnly>(&frontmatter) {
        Ok(parsed) => Ok(Some(parsed.id)),
        Err(error) => Err(error.into()),
    }
}

/// Recursively collect note files under `dir`, skipping reserved dirs and book files.
fn collect_note_files(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) -> CoreResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if path.is_dir() {
            if !layout::is_reserved_dir(&name) {
                collect_note_files(&path, root, out)?;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // Skip reserved top-level files (`_book.md`); they are not notes.
            if dir == root && layout::is_reserved_file(&name) {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

/// Categories are stored as frontmatter-only markdown files.
fn serialize_category(category: &Category) -> CoreResult<String> {
    let yaml = serde_yaml::to_string(category)?;
    Ok(format!("---\n{yaml}---\n"))
}

fn parse_category(content: &str) -> CoreResult<Category> {
    let (frontmatter, _body) = split_frontmatter(content)
        .ok_or_else(|| CoreError::parse("category file", "missing frontmatter"))?;
    let mut category: Category = serde_yaml::from_str(&frontmatter)?;
    // Single migration boundary for categories (mirrors `frontmatter::parse_note`).
    category.normalize();
    Ok(category)
}

/// Worlds, like categories, are stored as frontmatter-only markdown files in `_worlds/`.
fn serialize_world(world: &World) -> CoreResult<String> {
    let yaml = serde_yaml::to_string(world)?;
    Ok(format!("---\n{yaml}---\n"))
}

fn parse_world(content: &str) -> CoreResult<World> {
    let (frontmatter, _body) = split_frontmatter(content)
        .ok_or_else(|| CoreError::parse("world file", "missing frontmatter"))?;
    Ok(serde_yaml::from_str(&frontmatter)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    fn temp_store() -> (tempfile::TempDir, FsNoteStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsNoteStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn writes_reads_and_deletes_a_note() {
        let (_dir, store) = temp_store();
        let mut note = Note::new(ObjectType::Note, "first", "syllepsis_001");
        note.body = "body text".into();
        store.write_note(&note).unwrap();

        let read = store.read_note(&note.id).unwrap();
        assert_eq!(read.body, "body text");
        assert_eq!(store.read_all_notes().unwrap().len(), 1);

        store.delete_note(&note.id).unwrap();
        assert!(store.read_note(&note.id).is_err());
    }

    #[test]
    fn corrupt_note_is_skipped_and_reported_without_touching_the_file() {
        let (dir, store) = temp_store();
        let mut good = Note::new(ObjectType::Note, "good", "syllepsis_001");
        good.body = "fine".into();
        store.write_note(&good).unwrap();

        // Valid YAML with a valid id, but an unrecognized `type` — the id-only scan (which
        // ignores unknown fields) succeeds while the full `parse_note` fails.
        let corrupt_path = dir
            .path()
            .join("note-corrupt-01hqzzzzzzzzzzzzzzzzzzzzzz.md");
        let corrupt_bytes = "---\nid: note-corrupt-01hqzzzzzzzzzzzzzzzzzzzzzz\ntype: definitely_not_a_real_type\n---\nbody";
        fs::write(&corrupt_path, corrupt_bytes).unwrap();
        store.refresh().unwrap();

        let notes = store.read_all_notes().unwrap();
        assert_eq!(
            notes.len(),
            1,
            "the corrupt note must not appear in results"
        );
        assert_eq!(notes[0].body, "fine");

        let issues = store.load_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].path, "note-corrupt-01hqzzzzzzzzzzzzzzzzzzzzzz.md");
        assert_eq!(
            issues[0].ulid.as_deref(),
            Some("01hqzzzzzzzzzzzzzzzzzzzzzz")
        );

        // The file itself is left byte-identical on disk.
        assert_eq!(fs::read_to_string(&corrupt_path).unwrap(), corrupt_bytes);
    }

    #[test]
    fn note_with_unparseable_id_is_reported() {
        let (dir, store) = temp_store();
        let bad_id_path = dir.path().join("garbage.md");
        fs::write(&bad_id_path, "---\nid: not-a-real-id\n---\nbody").unwrap();
        store.refresh().unwrap();

        assert!(store.read_all_notes().unwrap().is_empty());
        let issues = store.load_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].path, "garbage.md");
        assert_eq!(issues[0].ulid, None);
    }

    #[test]
    fn retitle_renames_file_but_keeps_identity() {
        let (dir, store) = temp_store();
        let mut note = Note::new(ObjectType::Note, "old title", "syllepsis_001");
        store.write_note(&note).unwrap();
        let old_path = dir.path().join(layout::note_filename(&note.id));
        assert!(old_path.exists());

        note.retitle("a much newer title");
        store.write_note(&note).unwrap();
        let new_path = dir.path().join(layout::note_filename(&note.id));
        assert!(new_path.exists());
        assert!(!old_path.exists(), "stale slug filename should be removed");
        // Still resolvable by identity.
        assert_eq!(
            store.read_note(&note.id).unwrap().title,
            "a much newer title"
        );
    }

    #[test]
    fn resolves_after_external_rename() {
        let (dir, store) = temp_store();
        let mut note = Note::new(ObjectType::Note, "n", "syllepsis_001");
        note.body = "x".into();
        store.write_note(&note).unwrap();

        // Simulate an external tool renaming the file (identity stays in frontmatter).
        let original = dir.path().join(layout::note_filename(&note.id));
        let renamed = dir.path().join("totally-different-name.md");
        fs::rename(&original, &renamed).unwrap();
        store.refresh().unwrap();

        assert_eq!(store.read_note(&note.id).unwrap().body, "x");
    }

    #[test]
    fn external_delete_is_pruned_from_aggregate_reads() {
        let (dir, store) = temp_store();
        let mut note = Note::new(ObjectType::Note, "n", "syllepsis_001");
        note.body = "x".into();
        store.write_note(&note).unwrap();

        fs::remove_file(dir.path().join(layout::note_filename(&note.id))).unwrap();

        assert!(store.read_all_notes().unwrap().is_empty());
        assert!(matches!(
            store.read_note(&note.id).unwrap_err(),
            CoreError::NotFound(_)
        ));
    }

    #[test]
    fn categories_round_trip() {
        let (_dir, store) = temp_store();
        let mut cat = Category::new("electrical");
        cat.long_name = "Electrical Systems".into();
        store.write_category(&cat).unwrap();
        assert_eq!(store.categories().unwrap(), vec![cat.clone()]);
        assert_eq!(store.read_category("electrical").unwrap(), cat);
    }

    #[test]
    fn worlds_round_trip_and_delete() {
        let (_dir, store) = temp_store();
        let w = World::image("firstfloor", "First Floor", "drawing-1", (1024, 768));
        store.write_world(&w).unwrap();
        assert_eq!(store.worlds().unwrap(), vec![w]);
        store.delete_world("firstfloor").unwrap();
        assert!(store.worlds().unwrap().is_empty());
    }

    #[test]
    fn location_lookup_round_trips_and_ignores_world_files() {
        let (_dir, store) = temp_store();
        // A stored world file must not be misread as a lookup row (and vice versa).
        store
            .write_world(&World::image("firstfloor", "First Floor", "d-1", (10, 10)))
            .unwrap();
        let mut lookup = LocationLookup::new();
        lookup.upsert(crate::spatial::LookupEntry::new(
            "kitchen",
            "firstfloor",
            0.4,
            0.3,
        ));
        store.write_location_lookup(&lookup).unwrap();

        assert_eq!(
            store.read_location_lookup().unwrap().entries(),
            lookup.entries()
        );
        // The lookup CSV is not picked up by the world scan.
        assert_eq!(store.worlds().unwrap().len(), 1);
    }

    /// Category/world names come from imported packs and remote sync payloads, so a traversing
    /// name must be refused on *both* boundaries: the write path (no file outside the book) and
    /// the read path (no reading a file outside the book).
    #[test]
    fn hostile_category_names_are_rejected_on_write_and_read() {
        let enclosing = tempfile::tempdir().unwrap();
        let book_root = enclosing.path().join("book");
        fs::create_dir_all(&book_root).unwrap();
        // A file beside the book root that a traversing name would target.
        let outside = enclosing.path().join("secret.md");
        fs::write(&outside, "---\nname: secret\n---\n").unwrap();
        let store = FsNoteStore::open(&book_root).unwrap();

        // `_categories/../../secret` is exactly the decoy file beside the book root.
        for hostile in [
            "../../secret",
            "../secret",
            "../../etc/passwd",
            "sub/evil",
            "..",
            ".",
            "",
        ] {
            let mut category = Category::new("placeholder");
            category.name = hostile.to_string();
            assert!(
                matches!(
                    store.write_category(&category).unwrap_err(),
                    CoreError::InvalidId(_)
                ),
                "write_category must reject {hostile:?}"
            );
            assert!(
                matches!(
                    store.read_category(hostile).unwrap_err(),
                    CoreError::InvalidId(_)
                ),
                "read_category must reject {hostile:?}"
            );
            assert!(
                store.delete_category(hostile).is_err(),
                "delete_category must reject {hostile:?}"
            );
        }

        // Nothing new appeared beside the book, and the neighbouring file is untouched.
        let mut siblings: Vec<String> = fs::read_dir(enclosing.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        siblings.sort();
        assert_eq!(siblings, vec!["book".to_string(), "secret.md".to_string()]);
        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "---\nname: secret\n---\n"
        );
    }

    #[test]
    fn hostile_world_ids_are_rejected_on_write_and_delete() {
        let (_dir, store) = temp_store();
        let mut world = World::image("firstfloor", "First Floor", "drawing-1", (10, 10));
        world.id = "../../evil".into();
        assert!(matches!(
            store.write_world(&world).unwrap_err(),
            CoreError::InvalidId(_)
        ));
        assert!(matches!(
            store.delete_world("../../evil").unwrap_err(),
            CoreError::InvalidId(_)
        ));
    }

    #[test]
    fn missing_lookup_is_empty() {
        let (_dir, store) = temp_store();
        assert!(store.read_location_lookup().unwrap().entries().is_empty());
    }
}
