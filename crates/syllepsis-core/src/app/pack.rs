//! Application command surface for knowledge packs (core-concepts.md): exporting a curated set of
//! notes as a distributable [`Pack`], previewing what an incoming pack would do to the current
//! book (category mapping + per-note status), and importing it with the **local-modification
//! protection** that a version re-import must honor.
//!
//! Framework-agnostic operations over a [`Book`], like the rest of [`crate::app`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::id::NoteId;
use crate::model::metadata::Metadata;
use crate::model::{Category, Note};
use crate::pack::{Pack, PackManifest, PackNote};
use crate::storage::{Book, NoteStore};

/// What to put in an exported pack: the manifest fields plus the note selection. A note is
/// included if it carries one of `categories` **or** is named directly in `note_ids` (so an author
/// can export "everything tagged #permaculture, plus these three extras").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportSpec {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub note_ids: Vec<String>,
}

/// Assemble (but do not write) a pack from the book per `spec`. Pending-deletion notes are never
/// exported; the categories the selected notes use are bundled so the import side can recreate them.
pub fn build_pack(book: &Book, spec: &ExportSpec) -> CoreResult<Pack> {
    let wanted_categories: BTreeSet<&str> = spec.categories.iter().map(String::as_str).collect();
    let wanted_ids: BTreeSet<&str> = spec.note_ids.iter().map(String::as_str).collect();

    let selected: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| {
            n.metadata.lifecycle.marked_for_deletion_at.is_none()
                && (wanted_ids.contains(n.id.to_string().as_str())
                    || n.categories
                        .iter()
                        .any(|c| wanted_categories.contains(c.as_str())))
        })
        .collect();

    let used: BTreeSet<String> = selected
        .iter()
        .flat_map(|n| n.categories.iter().cloned())
        .collect();
    let categories: Vec<Category> = book
        .store
        .categories()?
        .into_iter()
        .filter(|c| used.contains(&c.name))
        .collect();

    let notes = selected.iter().map(PackNote::from_note).collect();
    Ok(Pack::new(
        PackManifest {
            id: spec.id.clone(),
            name: spec.name.clone(),
            version: spec.version.clone(),
            description: spec.description.clone(),
        },
        notes,
        categories,
    ))
}

/// Build a pack and write it to `path`, returning its manifest for a confirmation toast.
pub fn export_pack(book: &Book, spec: &ExportSpec, path: &Path) -> CoreResult<PackManifest> {
    let pack = build_pack(book, spec)?;
    pack.write_to(path)?;
    Ok(pack.manifest)
}

/// Read a pack file from disk (the import UI's first step).
pub fn read_pack(path: &Path) -> CoreResult<Pack> {
    Pack::read_from(path)
}

/// What importing one note would do, so the user can deselect before committing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// Not present in the book — a fresh add.
    New,
    /// Present from a previous import and unchanged locally — will be overwritten with this version.
    Update,
    /// Present and edited locally — protected: will be skipped on import.
    LocallyModified,
}

/// One incoming note's preview row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportNotePreview {
    pub id: String,
    pub title: String,
    pub status: ImportStatus,
}

/// A suggested mapping of an incoming category onto a local one (auto-suggesting near matches);
/// `suggested_local` is `None` when nothing matches and the import would create the category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryMapping {
    pub incoming: String,
    pub suggested_local: Option<String>,
}

/// The full preview the import view renders before the user commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub manifest: PackManifest,
    pub notes: Vec<ImportNotePreview>,
    pub category_suggestions: Vec<CategoryMapping>,
}

/// Dry-run a pack against the current book: classify each note (new / update / locally-modified)
/// and suggest a local category for each incoming category.
pub fn preview_import(book: &Book, pack: &Pack) -> CoreResult<ImportPreview> {
    let notes = pack
        .notes
        .iter()
        .map(|pn| {
            Ok(ImportNotePreview {
                id: pn.id.clone(),
                title: pn.title.clone(),
                status: note_import_status(book, &pn.id)?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;

    let local: Vec<String> = book
        .store
        .categories()?
        .into_iter()
        .map(|c| c.name)
        .collect();
    let incoming: BTreeSet<String> = pack
        .notes
        .iter()
        .flat_map(|n| n.categories.iter().cloned())
        .chain(pack.categories.iter().map(|c| c.name.clone()))
        .collect();
    let category_suggestions = incoming
        .into_iter()
        .map(|incoming| CategoryMapping {
            suggested_local: nearest_category(&incoming, &local),
            incoming,
        })
        .collect();

    Ok(ImportPreview {
        manifest: pack.manifest.clone(),
        notes,
        category_suggestions,
    })
}

/// Choices made in the import UI: which notes to actually import, and how to rename incoming
/// categories onto local ones (`incoming name → local name`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportOptions {
    pub selected_note_ids: Vec<String>,
    #[serde(default)]
    pub category_map: BTreeMap<String, String>,
}

/// What an import actually did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub skipped_locally_modified: Vec<String>,
    pub created_categories: Vec<String>,
}

/// Import the selected notes from `pack`, applying the category mapping and honoring the
/// local-modification protection. Existing pack notes the user has edited (`locally_modified`) are
/// skipped; unmodified ones are overwritten with the pack's content while **preserving the user's
/// organization** (sort position, location) and lifecycle flags. Referenced categories are created
/// locally when absent.
pub fn import_pack(book: &Book, pack: &Pack, options: &ImportOptions) -> CoreResult<ImportReport> {
    let selected: BTreeSet<&str> = options
        .selected_note_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut report = ImportReport::default();
    let mut needed_categories: BTreeSet<String> = BTreeSet::new();

    for pack_note in pack
        .notes
        .iter()
        .filter(|n| selected.contains(n.id.as_str()))
    {
        let id = NoteId::parse(&pack_note.id)?;
        let mapped = map_categories(&pack_note.categories, &options.category_map);
        needed_categories.extend(mapped.iter().cloned());

        let mut note = match book.store.read_note(&id) {
            Ok(existing) => {
                if existing.metadata.packs.locally_modified {
                    report.skipped_locally_modified.push(pack_note.id.clone());
                    continue;
                }
                existing // keep prior/location/lifecycle; content fields replaced below
            }
            Err(_) => new_pack_note(book, id, pack_note),
        };

        note.title = pack_note.title.clone();
        note.summary = pack_note.summary.clone();
        note.body = pack_note.body.clone();
        note.categories = mapped;
        record_membership(&mut note.metadata, &pack.manifest);
        note.metadata.dates.updated = chrono::Utc::now();
        book.save_note(&note)?;
        report.imported.push(pack_note.id.clone());
    }

    report.created_categories =
        ensure_categories(book, &needed_categories, pack, &options.category_map)?;
    Ok(report)
}

/// A fresh note shell carrying the pack note's identity; content is filled by the caller.
fn new_pack_note(book: &Book, id: NoteId, pack_note: &PackNote) -> Note {
    Note {
        id,
        object_type: pack_note.object_type,
        markdown_version: book.config.markdown.dialect_version.clone(),
        title: String::new(),
        summary: String::new(),
        body: String::new(),
        categories: Vec::new(),
        prior: None,
        location: None,
        metadata: Metadata::now(),
    }
}

/// Record (idempotently) that a note belongs to this pack at this version, clearing the
/// locally-modified flag since the note now matches the freshly imported pack content.
fn record_membership(metadata: &mut Metadata, manifest: &PackManifest) {
    if !metadata.packs.packs.contains(&manifest.id) {
        metadata.packs.packs.push(manifest.id.clone());
    }
    metadata.packs.pack_version = Some(manifest.version.clone());
    metadata.packs.locally_modified = false;
}

/// Classify how an incoming note id lands against the book's current state.
fn note_import_status(book: &Book, raw_id: &str) -> CoreResult<ImportStatus> {
    let Ok(id) = NoteId::parse(raw_id) else {
        return Ok(ImportStatus::New);
    };
    match book.store.read_note(&id) {
        Ok(existing) if existing.metadata.packs.locally_modified => {
            Ok(ImportStatus::LocallyModified)
        }
        Ok(_) => Ok(ImportStatus::Update),
        Err(_) => Ok(ImportStatus::New),
    }
}

/// Rename each category through the map (`incoming → local`); unmapped names pass through.
fn map_categories(categories: &[String], map: &BTreeMap<String, String>) -> Vec<String> {
    categories
        .iter()
        .map(|c| map.get(c).cloned().unwrap_or_else(|| c.clone()))
        .collect()
}

/// Create any referenced category that does not exist locally, preferring the pack's definition
/// (icon/heading) and falling back to a bare category. Returns the names created.
fn ensure_categories(
    book: &Book,
    needed: &BTreeSet<String>,
    pack: &Pack,
    map: &BTreeMap<String, String>,
) -> CoreResult<Vec<String>> {
    let mut created = Vec::new();
    for name in needed {
        if book.store.read_category(name).is_ok() {
            continue;
        }
        // Find the pack's definition for this category (under its original, pre-mapping name).
        let definition = pack
            .categories
            .iter()
            .find(|c| map.get(&c.name).map(|m| m == name).unwrap_or(false) || &c.name == name);
        let mut category = definition.cloned().unwrap_or_else(|| Category::new(name));
        category.name = name.clone();
        book.store.write_category(&category)?;
        created.push(name.clone());
    }
    Ok(created)
}

/// Nearest local category for an incoming name: an exact case-insensitive match wins.
fn nearest_category(incoming: &str, local: &[String]) -> Option<String> {
    local
        .iter()
        .find(|name| name.eq_ignore_ascii_case(incoming))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, get_note, update_note};
    use crate::model::ObjectType;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    fn add(book: &Book, title: &str, body: &str, cats: &[&str]) -> String {
        let mut n = create_note(book, ObjectType::Note, title, None).unwrap();
        n.body = body.into();
        n.categories = cats.iter().map(|c| c.to_string()).collect();
        update_note(book, n).unwrap().id
    }

    fn spec() -> ExportSpec {
        ExportSpec {
            id: "garden-pack".into(),
            name: "Garden Pack".into(),
            version: "1.0.0".into(),
            description: "garden notes".into(),
            categories: vec!["garden".into()],
            note_ids: vec![],
        }
    }

    #[test]
    fn export_selects_by_category_and_bundles_used_categories() {
        let (_d, book) = book();
        add(&book, "Compost", "greens and browns", &["garden"]);
        add(&book, "Wiring", "breaker panel", &["electrical"]);
        book.store.write_category(&Category::new("garden")).unwrap();

        let pack = build_pack(&book, &spec()).unwrap();
        assert_eq!(pack.notes.len(), 1);
        assert_eq!(pack.notes[0].title, "Compost");
        assert!(pack.categories.iter().any(|c| c.name == "garden"));
    }

    #[test]
    fn import_into_another_book_creates_notes_and_categories() {
        let (_d, source) = book();
        add(&source, "Compost", "greens and browns", &["garden"]);
        source
            .store
            .write_category(&Category::new("garden"))
            .unwrap();
        let pack = build_pack(&source, &spec()).unwrap();

        let (_d2, target) = book();
        let preview = preview_import(&target, &pack).unwrap();
        assert_eq!(preview.notes[0].status, ImportStatus::New);

        let options = ImportOptions {
            selected_note_ids: vec![pack.notes[0].id.clone()],
            category_map: BTreeMap::new(),
        };
        let report = import_pack(&target, &pack, &options).unwrap();
        assert_eq!(report.imported.len(), 1);
        assert!(report.created_categories.contains(&"garden".to_string()));

        let imported = get_note(&target, &pack.notes[0].id).unwrap();
        assert_eq!(imported.body, "greens and browns");
        assert!(imported
            .metadata
            .packs
            .packs
            .contains(&"garden-pack".to_string()));
        assert_eq!(
            imported.metadata.packs.pack_version.as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn reimport_overwrites_unmodified_but_protects_local_edits() {
        let (_d, source) = book();
        let note_id = add(&source, "Compost", "v1 body", &["garden"]);
        let pack_v1 = build_pack(&source, &spec()).unwrap();

        // Import v1 into the target.
        let (_d2, target) = book();
        let options = ImportOptions {
            selected_note_ids: vec![note_id.clone()],
            category_map: BTreeMap::new(),
        };
        import_pack(&target, &pack_v1, &options).unwrap();

        // The user edits the imported note locally → marks it locally_modified.
        let mut edited = get_note(&target, &note_id).unwrap();
        assert!(edited
            .metadata
            .packs
            .packs
            .contains(&"garden-pack".to_string()));
        edited.body = "user's own careful notes".into();
        update_note(&target, edited).unwrap();
        assert!(
            get_note(&target, &note_id)
                .unwrap()
                .metadata
                .packs
                .locally_modified
        );

        // Author ships v2 with new content for the same note.
        let mut v2_note = source
            .store
            .read_note(&NoteId::parse(&note_id).unwrap())
            .unwrap();
        v2_note.body = "v2 body".into();
        source.save_note(&v2_note).unwrap();
        let mut v2_spec = spec();
        v2_spec.version = "2.0.0".into();
        let pack_v2 = build_pack(&source, &v2_spec).unwrap();

        // Re-import: the locally-modified note is protected (skipped), not clobbered.
        let report = import_pack(&target, &pack_v2, &options).unwrap();
        assert_eq!(report.skipped_locally_modified, vec![note_id.clone()]);
        assert_eq!(
            get_note(&target, &note_id).unwrap().body,
            "user's own careful notes"
        );
    }

    #[test]
    fn category_mapping_renames_incoming_onto_local() {
        let (_d, source) = book();
        let id = add(&source, "Compost", "greens", &["garden"]);
        let pack = build_pack(&source, &spec()).unwrap();

        let (_d2, target) = book();
        target
            .store
            .write_category(&Category::new("plants"))
            .unwrap();

        // Suggest nothing for "garden" (no case-insensitive local match), but the user maps it.
        let preview = preview_import(&target, &pack).unwrap();
        let garden = preview
            .category_suggestions
            .iter()
            .find(|m| m.incoming == "garden")
            .unwrap();
        assert_eq!(garden.suggested_local, None);

        let mut map = BTreeMap::new();
        map.insert("garden".to_string(), "plants".to_string());
        let options = ImportOptions {
            selected_note_ids: vec![id.clone()],
            category_map: map,
        };
        import_pack(&target, &pack, &options).unwrap();

        let imported = get_note(&target, &id).unwrap();
        assert_eq!(imported.categories, vec!["plants".to_string()]);
    }
}
