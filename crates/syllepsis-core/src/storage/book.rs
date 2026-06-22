//! A **book** is a folder on disk. [`Book`] is the top-level handle the app works through: it
//! owns the note store, the per-book config, the book-level metadata, and the id registry
//! (collision backstop), and exposes the create/fork/save operations the command layer calls.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{CoreError, CoreResult};
use crate::id::NoteId;
use crate::markdown::frontmatter::split_frontmatter;
use crate::model::metadata::ForkInfo;
use crate::model::{Note, ObjectType};
use crate::storage::layout;
use crate::storage::registry::IdRegistry;
use crate::storage::store::{FsNoteStore, NoteStore};

/// Non-fatal diagnostics from opening a book directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookOpenWarning {
    /// Reserved top-level files absent at open time. Existing files are not compared against
    /// defaults, so normal config/schema drift across app versions does not trigger this warning.
    pub missing_reserved_files: Vec<String>,
}

impl BookOpenWarning {
    pub fn for_root(root: &Path) -> Option<BookOpenWarning> {
        let missing_reserved_files = [
            (layout::BOOK_META_FILE, layout::book_meta_path(root)),
            (layout::CONFIG_FILE, layout::config_path(root)),
        ]
        .into_iter()
        .filter_map(|(name, path)| (!path.exists()).then(|| name.to_string()))
        .collect::<Vec<_>>();

        (!missing_reserved_files.is_empty()).then_some(BookOpenWarning {
            missing_reserved_files,
        })
    }

    pub fn should_offer_create_here(&self) -> bool {
        self.missing_reserved_files.len() == 2
            && self
                .missing_reserved_files
                .iter()
                .any(|file| file == layout::BOOK_META_FILE)
            && self
                .missing_reserved_files
                .iter()
                .any(|file| file == layout::CONFIG_FILE)
    }
}

/// Book-level metadata stored in `_book.md` (object-types.md "Book-Level Metadata").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookMetadata {
    pub name: String,
    /// Preferred language (BCP-47-ish); guides LLM features and spell-check.
    #[serde(default = "default_language")]
    pub language: String,
    /// Optional real-world location (e.g. city of construction) for LLM lookups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Cover image/icon reference (SVG/JPG/PNG object id or path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// Friendly display names for opaque identity-provider handles (`handle → alias`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub author_aliases: std::collections::BTreeMap<String, String>,
}

fn default_language() -> String {
    "en".to_string()
}

impl BookMetadata {
    pub fn new(name: impl Into<String>) -> BookMetadata {
        BookMetadata {
            name: name.into(),
            language: default_language(),
            location: None,
            cover: None,
            author_aliases: Default::default(),
        }
    }
}

pub struct Book {
    pub root: PathBuf,
    pub metadata: BookMetadata,
    pub config: Config,
    pub store: FsNoteStore,
    /// Non-fatal open diagnostics the shell can surface before treating the folder as intentional.
    pub open_warning: Option<BookOpenWarning>,
    /// Machine-local directory holding downloaded ONNX models, shared across books. Runtime-only
    /// (never serialized — it is device-specific, not synced config); the shell sets it from the
    /// OS app-data path. `None` ⇒ no local models, so embeddings/LLM use their offline defaults.
    models_root: Option<PathBuf>,
    /// Collision backstop; interior-mutable so book operations can take `&self`.
    registry: Mutex<IdRegistry>,
}

impl Book {
    /// Create a new, empty book directory with its reserved layout and metadata files.
    pub fn create(root: impl Into<PathBuf>, name: impl Into<String>) -> CoreResult<Book> {
        Self::create_with_metadata(root, BookMetadata::new(name))
    }

    /// Create a new, empty book directory with caller-supplied metadata.
    pub fn create_with_metadata(
        root: impl Into<PathBuf>,
        metadata: BookMetadata,
    ) -> CoreResult<Book> {
        let root = root.into();
        ensure_new_book_root_available(&root)?;
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(layout::categories_dir(&root))?;
        std::fs::create_dir_all(layout::commentary_dir(&root))?;
        std::fs::create_dir_all(layout::worlds_dir(&root))?;
        std::fs::create_dir_all(layout::derived_dir(&root))?;
        std::fs::create_dir_all(layout::crdt_dir(&root))?;
        std::fs::create_dir_all(layout::sync_dir(&root))?;

        let config = Config::default();
        write_book_meta(&root, &metadata)?;
        write_config(&root, &config)?;
        // Git carries only the human-readable markdown ("public, partial rolling release",
        // platform-infra.md): ephemeral caches, device-local sync state, and the binary CRDT
        // sidecars are all kept out of commits. The cloud-drive SyncProvider still carries `_crdt`.
        std::fs::write(
            root.join(".gitignore"),
            format!(
                "{}/\n{}/\n{}/\n",
                layout::DERIVED_DIR,
                layout::SYNC_DIR,
                layout::CRDT_DIR
            ),
        )?;

        let store = FsNoteStore::open(&root)?;
        Ok(Book {
            root,
            metadata,
            config,
            store,
            open_warning: None,
            models_root: None,
            registry: Mutex::new(IdRegistry::default()),
        })
    }

    /// Open an existing book, loading metadata/config (defaulting when absent) and building
    /// the id registry from the notes already on disk.
    pub fn open(root: impl Into<PathBuf>) -> CoreResult<Book> {
        let root = root.into();
        let open_warning = BookOpenWarning::for_root(&root);
        let metadata =
            read_book_meta(&root)?.unwrap_or_else(|| BookMetadata::new(default_book_name(&root)));
        let config = read_config(&root)?.unwrap_or_default();
        let store = FsNoteStore::open(&root)?;
        let registry = IdRegistry::from_ids(store.note_ids()?.iter());
        Ok(Book {
            root,
            metadata,
            config,
            store,
            open_warning,
            models_root: None,
            registry: Mutex::new(registry),
        })
    }

    /// Point this book at the machine-local ONNX model directory (builder style). The shell calls
    /// this after open/create with the OS app-data models path; tests and the offline default
    /// leave it unset.
    pub fn with_models_root(mut self, models_root: impl Into<PathBuf>) -> Book {
        self.models_root = Some(models_root.into());
        self
    }

    /// The configured local-models directory, if any.
    pub fn models_root(&self) -> Option<&Path> {
        self.models_root.as_deref()
    }

    /// Create, persist, and return a fresh unsorted note with a registry-unique id.
    pub fn new_note(&self, object_type: ObjectType, title: impl Into<String>) -> CoreResult<Note> {
        let title = title.into();
        let id = self
            .registry
            .lock()
            .expect("registry poisoned")
            .mint(object_type.id_prefix(), &title);
        let mut note = Note::new(
            object_type,
            title,
            self.config.markdown.dialect_version.clone(),
        );
        note.id = id;
        self.store.write_note(&note)?;
        Ok(note)
    }

    /// Persist an edited note (registering its id if new, e.g. after an import).
    pub fn save_note(&self, note: &Note) -> CoreResult<()> {
        self.registry
            .lock()
            .expect("registry poisoned")
            .register(&note.id);
        self.store.write_note(note)
    }

    /// Fork a note: mint a new identity, record the lineage, and persist the copy.
    pub fn fork_note(&self, source: &NoteId) -> CoreResult<Note> {
        let mut forked = self.store.read_note(source)?;
        let new_id = self
            .registry
            .lock()
            .expect("registry poisoned")
            .mint(forked.object_type.id_prefix(), &forked.title);
        forked.id = new_id;
        forked.metadata.fork = Some(ForkInfo {
            forked_from: source.clone(),
            forked_at: Utc::now(),
        });
        forked.metadata.dates = crate::model::metadata::DateMetadata::now();
        self.store.write_note(&forked)?;
        Ok(forked)
    }

    /// Permanently remove a note and forget its identity. Also removes the note's CRDT sidecar
    /// (Phase 4) so a deletion does not leave an orphaned `_crdt/{ulid}.crdt` behind for sync.
    pub fn delete_note(&self, id: &NoteId) -> CoreResult<()> {
        self.store.delete_note(id)?;
        let _ = std::fs::remove_file(layout::crdt_sidecar_path(&self.root, id));
        self.registry.lock().expect("registry poisoned").remove(id);
        Ok(())
    }

    /// Persist book metadata changes.
    pub fn save_metadata(&self) -> CoreResult<()> {
        write_book_meta(&self.root, &self.metadata)
    }

    /// Persist config changes.
    pub fn save_config(&self) -> CoreResult<()> {
        write_config(&self.root, &self.config)
    }
}

fn default_book_name(root: &Path) -> String {
    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled Book")
        .to_string()
}

fn ensure_new_book_root_available(root: &Path) -> CoreResult<()> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(CoreError::InvalidBook(format!(
            "book path exists and is not a folder: {}",
            root.display()
        )));
    }
    if std::fs::read_dir(root)?.next().is_some() {
        return Err(CoreError::InvalidBook(format!(
            "refusing to create a book in a non-empty folder: {}",
            root.display()
        )));
    }
    Ok(())
}

fn write_book_meta(root: &Path, metadata: &BookMetadata) -> CoreResult<()> {
    let yaml = serde_yaml::to_string(metadata)?;
    std::fs::write(layout::book_meta_path(root), format!("---\n{yaml}---\n"))?;
    Ok(())
}

fn read_book_meta(root: &Path) -> CoreResult<Option<BookMetadata>> {
    let path = layout::book_meta_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    let (frontmatter, _body) = split_frontmatter(&content).unwrap_or((content, String::new()));
    Ok(Some(serde_yaml::from_str(&frontmatter)?))
}

fn write_config(root: &Path, config: &Config) -> CoreResult<()> {
    std::fs::write(layout::config_path(root), serde_yaml::to_string(config)?)?;
    Ok(())
}

fn read_config(root: &Path) -> CoreResult<Option<Config>> {
    let path = layout::config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_yaml::from_str(&std::fs::read_to_string(path)?)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_open_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my-book");
        {
            let book = Book::create(&path, "My Book").unwrap();
            assert_eq!(book.metadata.name, "My Book");
            assert!(layout::categories_dir(&path).exists());
            assert!(path.join(".gitignore").exists());
        }
        let reopened = Book::open(&path).unwrap();
        assert_eq!(reopened.metadata.name, "My Book");
        assert_eq!(reopened.config.markdown.dialect_version, "syllepsis_001");
        assert_eq!(reopened.open_warning, None);
    }

    #[test]
    fn create_refuses_non_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("existing.txt"), "not a book").unwrap();

        let err = match Book::create(dir.path(), "Downloads") {
            Ok(_) => panic!("non-empty directory should not be initialized"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("refusing to create a book in a non-empty folder"));
    }

    #[test]
    fn create_with_metadata_persists_book_details() {
        let dir = tempfile::tempdir().unwrap();
        let mut metadata = BookMetadata::new("Field Notes");
        metadata.language = "es".to_string();
        metadata.location = Some("Chicago".to_string());

        Book::create_with_metadata(dir.path().join("field-notes"), metadata).unwrap();

        let opened = Book::open(dir.path().join("field-notes")).unwrap();
        assert_eq!(opened.metadata.name, "Field Notes");
        assert_eq!(opened.metadata.language, "es");
        assert_eq!(opened.metadata.location.as_deref(), Some("Chicago"));
    }

    #[test]
    fn open_warns_when_book_marker_files_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let opened = Book::open(dir.path()).unwrap();
        let warning = opened
            .open_warning
            .expect("missing marker files should warn");
        assert_eq!(
            warning.missing_reserved_files,
            vec![layout::BOOK_META_FILE, layout::CONFIG_FILE]
        );
        assert!(warning.should_offer_create_here());
        assert_eq!(
            opened.metadata.name,
            dir.path().file_name().unwrap().to_string_lossy()
        );
    }

    #[test]
    fn open_warns_only_about_absent_marker_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            layout::book_meta_path(dir.path()),
            "---\nname: Existing\n---\n",
        )
        .unwrap();

        let opened = Book::open(dir.path()).unwrap();
        let warning = opened.open_warning.expect("missing config should warn");
        assert_eq!(warning.missing_reserved_files, vec![layout::CONFIG_FILE]);
        assert!(!warning.should_offer_create_here());
    }

    #[test]
    fn new_note_is_registered_and_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "B").unwrap();
        let note = book.new_note(ObjectType::Note, "hello").unwrap();
        assert_eq!(book.store.read_note(&note.id).unwrap().id, note.id);
    }

    #[test]
    fn fork_creates_new_identity_with_lineage() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "B").unwrap();
        let original = book.new_note(ObjectType::Note, "source").unwrap();
        let forked = book.fork_note(&original.id).unwrap();
        assert_ne!(forked.id.ulid(), original.id.ulid());
        assert_eq!(
            forked.metadata.fork.as_ref().unwrap().forked_from,
            original.id
        );
    }
}
