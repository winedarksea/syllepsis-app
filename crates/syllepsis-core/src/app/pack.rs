//! Application command surface for knowledge packs (core-concepts.md): exporting a curated set of
//! notes as a distributable [`Pack`], previewing what an incoming pack would do to the current
//! book (category mapping + per-note status), and importing it with baseline-grounded modification
//! detection and a per-note resolution wizard for locally-modified notes.
//!
//! Framework-agnostic operations over a [`Book`], like the rest of [`crate::app`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app::pack_manifest::{BookPackManifest, NoteBaseline};
use crate::error::CoreResult;
use crate::id::NoteId;
use crate::model::metadata::Metadata;
use crate::model::{Category, CommentaryKind, CommentaryMetadata, CommentarySource, Note, ObjectType};
use crate::model::prior::PriorRef;
use crate::pack::{ExportKind, Pack, PackCommentary, PackManifest, PackNote};
use crate::storage::{layout, Book, NoteStore};

/// What to put in an exported pack: the manifest fields plus the note selection. A note is
/// included if it carries one of `categories` **or** is named directly in `note_ids` (so an author
/// can export "everything tagged #permaculture, plus these three extras"). When `export_all` is
/// true the category/id filters are ignored and every non-deleted note is included.
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
    #[serde(default)]
    pub export_all: bool,
    /// When true, commentary children of exported notes are bundled into `pack.commentary`.
    /// Default false — commentary is private and off by default.
    #[serde(default)]
    pub include_commentary: bool,
}

/// Assemble (but do not write) a pack from the book per `spec`. Pending-deletion notes are never
/// exported; the categories the selected notes use are bundled so the import side can recreate them.
/// When `spec.export_all` is true all non-deleted notes are included and `export_kind` is set to
/// `Book`; otherwise the category/id filter applies and `export_kind` is `Pack`.
pub fn build_pack(book: &Book, spec: &ExportSpec) -> CoreResult<Pack> {
    let all_notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| note.object_type != ObjectType::Commentary)
        .collect();

    let selected: Vec<Note> = if spec.export_all {
        all_notes
            .into_iter()
            .filter(|n| n.metadata.lifecycle.marked_for_deletion_at.is_none())
            .collect()
    } else {
        let wanted_categories: BTreeSet<&str> =
            spec.categories.iter().map(String::as_str).collect();
        let wanted_ids: BTreeSet<&str> = spec.note_ids.iter().map(String::as_str).collect();
        all_notes
            .into_iter()
            .filter(|n| {
                n.metadata.lifecycle.marked_for_deletion_at.is_none()
                    && (wanted_ids.contains(n.id.to_string().as_str())
                        || n.categories
                            .iter()
                            .any(|c| wanted_categories.contains(c.as_str())))
            })
            .collect()
    };

    let selected_ids: BTreeSet<String> =
        selected.iter().map(|n| n.id.to_string()).collect();

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

    let export_kind = if spec.export_all {
        ExportKind::Book
    } else {
        ExportKind::Pack
    };

    // Build PackNotes with refined priors (kept only when the target is also in the pack).
    let notes: Vec<PackNote> = selected
        .iter()
        .map(|note| {
            let mut pn = PackNote::from_note(note);
            pn.prior = note.prior.as_ref().and_then(|edge| {
                let keep = match &edge.target {
                    PriorRef::Note(target_id) => selected_ids.contains(&target_id.to_string()),
                    PriorRef::Category(name) => used.contains(name),
                };
                if keep { Some(edge.clone()) } else { None }
            });
            pn
        })
        .collect();

    let mut pack = Pack::new(
        PackManifest {
            id: spec.id.clone(),
            name: spec.name.clone(),
            version: spec.version.clone(),
            description: spec.description.clone(),
            export_kind,
        },
        notes,
        categories,
    );

    // Optionally bundle commentary children of exported notes.
    if spec.include_commentary {
        let parent_ids = &selected_ids;
        for commentary_note in book.read_all_commentary_notes()? {
            let Some(meta) = &commentary_note.commentary else { continue };
            if !parent_ids.contains(&meta.parent_note_id.to_string()) {
                continue;
            }
            if commentary_note.metadata.lifecycle.marked_for_deletion_at.is_some() {
                continue;
            }
            pack.commentary.push(PackCommentary {
                id: commentary_note.id.to_string(),
                title: commentary_note.title.clone(),
                body: commentary_note.body.clone(),
                commentary: meta.clone(),
            });
        }
    }

    Ok(pack)
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
    /// Present and edited locally — requires an explicit resolution from the user.
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
                status: note_import_status(book, &pack.manifest.id, &pn.id)?,
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

/// How to handle a locally-modified note when re-importing a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteResolution {
    /// Replace the local note's content with the pack's version and reset the baseline.
    Overwrite,
    /// 3-way CRDT merge (Loro): load the ancestor snapshot, apply the pack body as the remote
    /// edit, merge into the local live sidecar. Requires Loro feature.
    Merge,
    /// Leave the local note untouched; create a `Proposal` commentary child with the pack body.
    Commentary,
    /// Create a new note from the pack content; the original note becomes the user's fork.
    Duplicate,
    /// Do nothing — skip this note.
    Skip,
}

/// Choices made in the import UI: which notes to actually import, how to rename incoming
/// categories onto local ones, and per-note resolutions for locally-modified notes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportOptions {
    pub selected_note_ids: Vec<String>,
    #[serde(default)]
    pub category_map: BTreeMap<String, String>,
    /// Resolution for each locally-modified note. Absent = Skip (default).
    #[serde(default)]
    pub resolutions: BTreeMap<String, NoteResolution>,
}

/// What an import actually did — per-action counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub skipped_locally_modified: Vec<String>,
    pub created_categories: Vec<String>,
    pub overwritten: Vec<String>,
    pub merged: Vec<String>,
    pub commentary_created: Vec<String>,
    pub duplicated: Vec<String>,
}

/// Import the selected notes from `pack`, applying the category mapping and honoring the
/// per-note resolution for locally-modified notes. Auto-overwrites unmodified notes.
pub fn import_pack(book: &Book, pack: &Pack, options: &ImportOptions) -> CoreResult<ImportReport> {
    let selected: BTreeSet<&str> = options
        .selected_note_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut report = ImportReport::default();
    let mut needed_categories: BTreeSet<String> = BTreeSet::new();

    // Load or create the manifest for this pack.
    let mut manifest = BookPackManifest::load(&book.root, &pack.manifest.id)?;
    manifest.pack_id = pack.manifest.id.clone();
    manifest.version = pack.manifest.version.clone();
    manifest.imported_at = chrono::Utc::now().to_rfc3339();

    for pack_note in pack
        .notes
        .iter()
        .filter(|n| selected.contains(n.id.as_str()))
    {
        let id = NoteId::parse(&pack_note.id)?;
        let mapped = map_categories(&pack_note.categories, &options.category_map);
        needed_categories.extend(mapped.iter().cloned());

        let status = note_import_status(book, &pack.manifest.id, &pack_note.id)?;

        match status {
            ImportStatus::New => {
                // Create the note and record baseline.
                let mut note = new_pack_note(book, id.clone(), pack_note);
                note.title = pack_note.title.clone();
                note.summary = pack_note.summary.clone();
                note.body = pack_note.body.clone();
                note.categories = mapped;
                // Apply prior if it points at an already-imported or in-this-pack note.
                // The prior is already refined at export time; we restore it directly.
                note.prior = pack_note.prior.clone();
                record_membership(&mut note.metadata, &pack.manifest);
                note.metadata.dates.updated = chrono::Utc::now();
                book.save_note(&note)?;
                let baseline = capture_baseline(book, &id, &pack_note.body, &pack.manifest.version)?;
                manifest.notes.insert(pack_note.id.clone(), baseline);
                report.imported.push(pack_note.id.clone());
            }

            ImportStatus::Update => {
                // Unmodified re-import: overwrite content, refresh baseline.
                let mut note = book.store.read_note(&id)?;
                note.title = pack_note.title.clone();
                note.summary = pack_note.summary.clone();
                note.body = pack_note.body.clone();
                note.categories = mapped;
                record_membership(&mut note.metadata, &pack.manifest);
                note.metadata.dates.updated = chrono::Utc::now();
                book.save_note(&note)?;
                let baseline = capture_baseline(book, &id, &pack_note.body, &pack.manifest.version)?;
                manifest.notes.insert(pack_note.id.clone(), baseline);
                report.imported.push(pack_note.id.clone());
            }

            ImportStatus::LocallyModified => {
                let resolution = options
                    .resolutions
                    .get(&pack_note.id)
                    .copied()
                    .unwrap_or(NoteResolution::Skip);

                match resolution {
                    NoteResolution::Skip => {
                        report.skipped_locally_modified.push(pack_note.id.clone());
                    }

                    NoteResolution::Overwrite => {
                        let mut note = book.store.read_note(&id)?;
                        note.title = pack_note.title.clone();
                        note.summary = pack_note.summary.clone();
                        note.body = pack_note.body.clone();
                        note.categories = mapped;
                        record_membership(&mut note.metadata, &pack.manifest);
                        note.metadata.dates.updated = chrono::Utc::now();
                        book.save_note(&note)?;
                        let baseline = capture_baseline(book, &id, &pack_note.body, &pack.manifest.version)?;
                        manifest.notes.insert(pack_note.id.clone(), baseline);
                        report.overwritten.push(pack_note.id.clone());
                    }

                    NoteResolution::Merge => {
                        let merged_body = try_crdt_merge(book, &id, &pack_note.id, &pack_note.body, &manifest)?;
                        let mut note = book.store.read_note(&id)?;
                        note.body = merged_body;
                        note.categories = mapped;
                        record_membership(&mut note.metadata, &pack.manifest);
                        note.metadata.dates.updated = chrono::Utc::now();
                        book.save_note(&note)?;
                        let baseline = capture_baseline(book, &id, &pack_note.body, &pack.manifest.version)?;
                        manifest.notes.insert(pack_note.id.clone(), baseline);
                        report.merged.push(pack_note.id.clone());
                    }

                    NoteResolution::Commentary => {
                        // Leave the local note untouched; surface the pack body as a Proposal.
                        crate::app::commentary::create_commentary(
                            book,
                            &pack_note.id,
                            CommentaryKind::Proposal,
                            &pack_note.body,
                        )?;
                        report.commentary_created.push(pack_note.id.clone());
                        // Baseline unchanged.
                    }

                    NoteResolution::Duplicate => {
                        // Create a new note carrying the pack content.
                        let new_id = NoteId::generate("note", &pack_note.title);
                        let mut dup = new_pack_note(book, new_id.clone(), pack_note);
                        dup.title = pack_note.title.clone();
                        dup.summary = pack_note.summary.clone();
                        dup.body = pack_note.body.clone();
                        dup.categories = mapped;
                        record_membership(&mut dup.metadata, &pack.manifest);
                        dup.metadata.dates.updated = chrono::Utc::now();
                        book.save_note(&dup)?;
                        let baseline = capture_baseline(book, &new_id, &pack_note.body, &pack.manifest.version)?;
                        manifest.notes.insert(new_id.to_string(), baseline);

                        // Remove the original note from pack membership so it becomes a fork.
                        if let Ok(mut original) = book.store.read_note(&id) {
                            original.metadata.packs.packs.retain(|p| p != &pack.manifest.id);
                            original.metadata.packs.pack_version = None;
                            original.metadata.packs.locally_modified = false;
                            book.save_note(&original)?;
                        }
                        manifest.notes.remove(&pack_note.id);
                        report.duplicated.push(pack_note.id.clone());
                    }
                }
            }
        }
    }

    // Import bundled commentary (if any), creating children for imported parents.
    for pack_commentary in &pack.commentary {
        let parent_id_str = pack_commentary.commentary.parent_note_id.to_string();
        // Only recreate if the parent was imported/present.
        let Ok(parent_id) = NoteId::parse(&parent_id_str) else { continue };
        if book.store.read_note(&parent_id).is_err() {
            continue;
        }
        // Skip if we already have a commentary with this id.
        if let Ok(commentary_id) = NoteId::parse(&pack_commentary.id) {
            if book.read_commentary_note(&commentary_id).is_ok() {
                continue;
            }
        }
        // Build a new commentary note preserving the metadata.
        let meta = CommentaryMetadata::new(
            pack_commentary.commentary.parent_note_id.clone(),
            pack_commentary.commentary.kind,
            CommentarySource::User,
        );
        if let Ok(mut commentary_note) = book.new_commentary_note(pack_commentary.title.clone(), meta) {
            commentary_note.body = pack_commentary.body.clone();
            let _ = book.save_commentary_note(&commentary_note);
        }
    }

    // Persist the updated manifest.
    manifest.save(&book.root)?;

    report.created_categories =
        ensure_categories(book, &needed_categories, pack, &options.category_map)?;
    Ok(report)
}

/// Create a brand-new book at `root` and import every note from `pack` into it.
///
/// If `Book::create` succeeds but `import_pack` fails the empty book folder remains on disk
/// (matches `create_book_in_parent` behavior — caller must clean up on error).
pub fn import_pack_as_new_book(root: &Path, name: &str, pack: &Pack) -> CoreResult<Book> {
    let book = Book::create(root, name)?;
    let all_ids: Vec<String> = pack.notes.iter().map(|n| n.id.clone()).collect();
    let options = ImportOptions {
        selected_note_ids: all_ids,
        category_map: Default::default(),
        resolutions: Default::default(),
    };
    import_pack(&book, pack, &options)?;
    Ok(book)
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
        asset: None,
        commentary: None,
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

/// Classify how an incoming note id lands against the book's current state. Uses the
/// `BookPackManifest` baseline hash for accurate modification detection.
fn note_import_status(book: &Book, pack_id: &str, raw_id: &str) -> CoreResult<ImportStatus> {
    let Ok(id) = NoteId::parse(raw_id) else {
        return Ok(ImportStatus::New);
    };
    let Ok(existing) = book.store.read_note(&id) else {
        return Ok(ImportStatus::New);
    };
    // Use manifest baseline if available; fall back to the eager flag for back-compat.
    let manifest = BookPackManifest::load(&book.root, pack_id)?;
    if let Some(baseline) = manifest.notes.get(raw_id) {
        let current_hash = sha256_hex(&existing.body);
        if current_hash != baseline.base_body_sha256 {
            return Ok(ImportStatus::LocallyModified);
        }
        return Ok(ImportStatus::Update);
    }
    // No baseline yet — fall back to the eager flag.
    if existing.metadata.packs.locally_modified {
        return Ok(ImportStatus::LocallyModified);
    }
    if existing.metadata.packs.packs.is_empty() {
        return Ok(ImportStatus::New);
    }
    Ok(ImportStatus::Update)
}

/// Capture a baseline for a note: seed a CRDT doc from `body`, write its snapshot as the note's
/// `_crdt/{ulid}.crdt` sidecar (so the live doc descends from the baseline for clean Loro merges
/// later), and return the `NoteBaseline` to store in the manifest.
fn capture_baseline(
    book: &Book,
    id: &NoteId,
    body: &str,
    pack_version: &str,
) -> CoreResult<NoteBaseline> {
    let backend = crate::crdt::select_crdt_backend(&book.config.sync);
    let actor = crate::sync::actor_id_for(&book.root)?;
    let doc = backend.new_document(&actor, body);
    let snapshot = doc.snapshot()?;
    let sidecar = layout::crdt_sidecar_path(&book.root, id);
    if let Some(parent) = sidecar.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&sidecar, &snapshot)?;
    Ok(NoteBaseline {
        pack_version: pack_version.to_string(),
        base_body_sha256: sha256_hex(body),
        crdt_backend: backend.name().to_string(),
        base_crdt_snapshot_b64: base64::engine::general_purpose::STANDARD.encode(&snapshot),
    })
}

/// Perform a 3-way CRDT merge for a locally-modified note. Mirrors `try_crdt_merge_body` in
/// `commentary.rs`: load the ancestor from the stored baseline snapshot, set the pack body as the
/// remote edit, merge into the local live sidecar.
fn try_crdt_merge(
    book: &Book,
    id: &NoteId,
    raw_id: &str,
    pack_body: &str,
    manifest: &BookPackManifest,
) -> CoreResult<String> {
    // Retrieve the stored baseline for this note.
    let baseline = match manifest.notes.get(raw_id) {
        Some(b) if !b.base_crdt_snapshot_b64.is_empty() => b,
        _ => {
            // No Loro baseline — fall back to the pack body (treated as Overwrite).
            return Ok(pack_body.to_string());
        }
    };
    if baseline.crdt_backend != crate::crdt::LORO_BACKEND {
        return Ok(pack_body.to_string());
    }
    let backend = crate::crdt::select_crdt_backend(&book.config.sync);
    if backend.name() != crate::crdt::LORO_BACKEND {
        // Loro not compiled/active — fall back to pack body.
        return Ok(pack_body.to_string());
    }
    let actor = crate::sync::actor_id_for(&book.root)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&baseline.base_crdt_snapshot_b64)
        .map_err(|e| crate::error::CoreError::InvalidBook(format!("decode pack baseline: {e}")))?;

    // Build a "proposal" doc from the ancestor, then set it to the pack body.
    let mut proposal_doc = backend.load_document(&actor, &decoded)?;
    proposal_doc.set_text(pack_body);

    // Load the live local sidecar and merge.
    let sidecar = layout::crdt_sidecar_path(&book.root, id);
    let mut current_doc = if sidecar.exists() {
        backend.load_document(&actor, &std::fs::read(sidecar)?)?
    } else {
        let note = book.store.read_note(id)?;
        backend.new_document(&actor, &note.body)
    };
    current_doc.merge(&proposal_doc.snapshot()?)?;
    Ok(current_doc.text())
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

fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, get_note, update_note};
    use crate::model::ObjectType;
    use crate::pack::ExportKind;

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
            export_all: false,
            include_commentary: false,
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
            resolutions: BTreeMap::new(),
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
            resolutions: BTreeMap::new(),
        };
        import_pack(&target, &pack_v1, &options).unwrap();

        // The user edits the imported note locally.
        let mut edited = get_note(&target, &note_id).unwrap();
        assert!(edited
            .metadata
            .packs
            .packs
            .contains(&"garden-pack".to_string()));
        edited.body = "user's own careful notes".into();
        update_note(&target, edited).unwrap();
        // The note is locally modified (hash differs from baseline).
        let status = note_import_status(&target, "garden-pack", &note_id).unwrap();
        assert_eq!(status, ImportStatus::LocallyModified);
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

        // Re-import with default resolution (Skip) — locally-modified note is protected.
        let report = import_pack(&target, &pack_v2, &options).unwrap();
        assert_eq!(report.skipped_locally_modified, vec![note_id.clone()]);
        assert_eq!(
            get_note(&target, &note_id).unwrap().body,
            "user's own careful notes"
        );
    }

    #[test]
    fn reimport_with_overwrite_resolution_replaces_local_edit() {
        let (_d, source) = book();
        let note_id = add(&source, "Compost", "v1 body", &["garden"]);
        let pack_v1 = build_pack(&source, &spec()).unwrap();

        let (_d2, target) = book();
        let base_options = ImportOptions {
            selected_note_ids: vec![note_id.clone()],
            category_map: BTreeMap::new(),
            resolutions: BTreeMap::new(),
        };
        import_pack(&target, &pack_v1, &base_options).unwrap();

        // User edits the note.
        let mut edited = get_note(&target, &note_id).unwrap();
        edited.body = "local edit".into();
        update_note(&target, edited).unwrap();

        // Re-import v2 with Overwrite resolution.
        let mut v2_note = source.store.read_note(&NoteId::parse(&note_id).unwrap()).unwrap();
        v2_note.body = "v2 body".into();
        source.save_note(&v2_note).unwrap();
        let mut v2_spec = spec();
        v2_spec.version = "2.0.0".into();
        let pack_v2 = build_pack(&source, &v2_spec).unwrap();

        let mut overwrite_options = base_options.clone();
        overwrite_options.resolutions.insert(note_id.clone(), NoteResolution::Overwrite);
        let report = import_pack(&target, &pack_v2, &overwrite_options).unwrap();
        assert!(report.overwritten.contains(&note_id));
        assert_eq!(get_note(&target, &note_id).unwrap().body, "v2 body");
    }

    #[test]
    fn reimport_with_duplicate_resolution_creates_second_note() {
        let (_d, source) = book();
        let note_id = add(&source, "Compost", "v1 body", &["garden"]);
        let pack_v1 = build_pack(&source, &spec()).unwrap();

        let (_d2, target) = book();
        let base_options = ImportOptions {
            selected_note_ids: vec![note_id.clone()],
            category_map: BTreeMap::new(),
            resolutions: BTreeMap::new(),
        };
        import_pack(&target, &pack_v1, &base_options).unwrap();

        let mut edited = get_note(&target, &note_id).unwrap();
        edited.body = "my fork".into();
        update_note(&target, edited).unwrap();

        let mut v2_note = source.store.read_note(&NoteId::parse(&note_id).unwrap()).unwrap();
        v2_note.body = "v2 body".into();
        source.save_note(&v2_note).unwrap();
        let mut v2_spec = spec();
        v2_spec.version = "2.0.0".into();
        let pack_v2 = build_pack(&source, &v2_spec).unwrap();

        let mut dup_options = base_options.clone();
        dup_options.resolutions.insert(note_id.clone(), NoteResolution::Duplicate);
        let report = import_pack(&target, &pack_v2, &dup_options).unwrap();
        assert!(report.duplicated.contains(&note_id));
        // Original note still has the user's fork body.
        assert_eq!(get_note(&target, &note_id).unwrap().body, "my fork");
        // A second note now exists with the pack v2 body.
        let all = target.store.read_all_notes().unwrap();
        assert!(all.iter().any(|n| n.body == "v2 body"));
    }

    #[test]
    fn export_keeps_in_pack_prior_drops_out_of_pack_prior() {
        use crate::app::commands::update_note;
        use crate::model::prior::{PriorEdge, PriorRef, PriorKind};

        let (_d, book) = book();
        let a_id_str = add(&book, "NoteA", "body a", &["garden"]);
        let b_id_str = add(&book, "NoteB", "body b", &["garden"]);
        let outside_id_str = add(&book, "Outside", "body outside", &["electrical"]);

        // Set NoteB to follow NoteA (both in pack).
        let a_id = NoteId::parse(&a_id_str).unwrap();
        let mut note_b = get_note(&book, &b_id_str).unwrap();
        note_b.prior = Some(PriorEdge { target: PriorRef::Note(a_id.clone()), kind: PriorKind::NewParagraph });
        update_note(&book, note_b).unwrap();

        // Set NoteA to follow Outside (out-of-pack).
        let outside_id = NoteId::parse(&outside_id_str).unwrap();
        let mut note_a = get_note(&book, &a_id_str).unwrap();
        note_a.prior = Some(PriorEdge { target: PriorRef::Note(outside_id.clone()), kind: PriorKind::NewParagraph });
        update_note(&book, note_a).unwrap();

        book.store.write_category(&Category::new("garden")).unwrap();
        let pack = build_pack(&book, &spec()).unwrap();
        assert_eq!(pack.notes.len(), 2);

        // NoteB's prior (→ NoteA, in-pack) should be preserved.
        let pn_b = pack.notes.iter().find(|n| n.id == b_id_str).unwrap();
        assert!(pn_b.prior.is_some(), "in-pack prior should be kept");

        // NoteA's prior (→ Outside, not in pack) should be dropped.
        let pn_a = pack.notes.iter().find(|n| n.id == a_id_str).unwrap();
        assert!(pn_a.prior.is_none(), "out-of-pack prior should be dropped");
    }

    #[test]
    fn include_commentary_round_trips() {
        let (_d, source) = book();
        let note_id = add(&source, "Compost", "body", &["garden"]);
        // Create a commentary child.
        crate::app::commentary::create_commentary(
            &source,
            &note_id,
            CommentaryKind::Proposal,
            "proposed edit",
        )
        .unwrap();
        source.store.write_category(&Category::new("garden")).unwrap();

        let mut s = spec();
        s.include_commentary = true;
        let pack = build_pack(&source, &s).unwrap();
        assert_eq!(pack.commentary.len(), 1);
        assert_eq!(pack.commentary[0].body, "proposed edit");

        // A pack without include_commentary should omit it.
        let pack_no_comm = build_pack(&source, &spec()).unwrap();
        assert!(pack_no_comm.commentary.is_empty());
    }

    #[test]
    fn import_pack_as_new_book_creates_book_with_all_notes() {
        let (_d, source) = book();
        add(&source, "Compost", "greens and browns", &["garden"]);
        add(&source, "Wiring", "breaker panel", &["electrical"]);
        source
            .store
            .write_category(&Category::new("garden"))
            .unwrap();
        source
            .store
            .write_category(&Category::new("electrical"))
            .unwrap();
        let pack = build_pack(
            &source,
            &ExportSpec {
                id: "full".into(),
                name: "Full".into(),
                version: "1.0.0".into(),
                export_all: true,
                ..Default::default()
            },
        )
        .unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let book_path = dest_dir.path().join("new-book");
        let new_book =
            crate::app::pack::import_pack_as_new_book(&book_path, "New Book", &pack).unwrap();
        let notes = new_book.store.read_all_notes().unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.title == "Compost"));
        assert!(notes.iter().any(|n| n.title == "Wiring"));
        let cats: Vec<_> = new_book
            .store
            .categories()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(cats.contains(&"garden".to_string()));
        assert!(cats.contains(&"electrical".to_string()));
    }

    #[test]
    fn export_all_exports_every_non_deleted_note() {
        let (_d, book) = book();
        add(&book, "Compost", "greens and browns", &["garden"]);
        add(&book, "Wiring", "breaker panel", &["electrical"]);
        let deleted_id = add(&book, "Draft", "wip", &["scratch"]);
        let nid = NoteId::parse(&deleted_id).unwrap();
        let mut draft = book.store.read_note(&nid).unwrap();
        draft.metadata.lifecycle.marked_for_deletion_at = Some(chrono::Utc::now());
        book.save_note(&draft).unwrap();

        let all_spec = ExportSpec {
            id: "full-book".into(),
            name: "Full Book".into(),
            version: "1.0.0".into(),
            export_all: true,
            ..Default::default()
        };
        let pack = build_pack(&book, &all_spec).unwrap();
        assert_eq!(pack.notes.len(), 2, "deleted note must be excluded");
        assert!(pack.notes.iter().any(|n| n.title == "Compost"));
        assert!(pack.notes.iter().any(|n| n.title == "Wiring"));
        assert_eq!(pack.manifest.export_kind, ExportKind::Book);
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
            resolutions: BTreeMap::new(),
        };
        import_pack(&target, &pack, &options).unwrap();

        let imported = get_note(&target, &id).unwrap();
        assert_eq!(imported.categories, vec!["plants".to_string()]);
    }
}
