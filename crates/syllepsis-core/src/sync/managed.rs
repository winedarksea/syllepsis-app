//! Managed cloud sync over an append-only Loro patch log.
//!
//! This engine deliberately does not use [`SyncEngine`](super::SyncEngine): API-backed cloud
//! stores should not race on one overwritten book file. Each note gets append-only Loro updates
//! named by timestamp/device id, plus occasional snapshots for cheap bootstrap.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::SyncConfig;
use crate::crdt::{select_crdt_backend, ActorId};
use crate::error::{CoreError, CoreResult};
use crate::model::Note;
use crate::storage::{layout, Book, NoteStore};
use crate::sync::{actor_id_for, append_activity, content_revision, SyncActivityEvent};

const MANIFEST_PATH: &str = "manifest.json";
const STATE_PREFIX: &str = "managed-cloud";
const EMPTY_VERSION_VECTOR_JSON: &str = "{}";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedObjectEntry {
    pub path: String,
    pub size: u64,
}

pub trait ManagedObjectStore {
    fn list(&self, prefix: &str) -> CoreResult<Vec<ManagedObjectEntry>>;
    fn get(&self, path: &str) -> CoreResult<Vec<u8>>;
    fn put(&mut self, path: &str, bytes: &[u8]) -> CoreResult<()>;
    fn delete(&mut self, path: &str) -> CoreResult<()>;
}

#[derive(Debug, Default, Clone)]
pub struct MemoryManagedObjectStore {
    objects: BTreeMap<String, Vec<u8>>,
}

impl MemoryManagedObjectStore {
    pub fn objects(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.objects
    }
}

impl ManagedObjectStore for MemoryManagedObjectStore {
    fn list(&self, prefix: &str) -> CoreResult<Vec<ManagedObjectEntry>> {
        Ok(self
            .objects
            .iter()
            .filter(|(path, _)| path.starts_with(prefix))
            .map(|(path, bytes)| ManagedObjectEntry {
                path: path.clone(),
                size: bytes.len() as u64,
            })
            .collect())
    }

    fn get(&self, path: &str) -> CoreResult<Vec<u8>> {
        self.objects
            .get(path)
            .cloned()
            .ok_or_else(|| CoreError::Sync(format!("cloud object not found: {path}")))
    }

    fn put(&mut self, path: &str, bytes: &[u8]) -> CoreResult<()> {
        self.objects.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    fn delete(&mut self, path: &str) -> CoreResult<()> {
        self.objects.remove(path);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookManifest {
    pub schema_version: u32,
    pub book_id: String,
    pub name: String,
    pub updated_at: chrono::DateTime<Utc>,
    pub notes: Vec<CloudNoteRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudNoteRecord {
    pub note: Note,
    pub note_path: String,
    pub latest_snapshot_path: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedCloudSyncState {
    pub provider: String,
    pub exported_version_vectors: BTreeMap<String, String>,
    pub seen_patches: BTreeSet<String>,
    pub latest_snapshots: BTreeMap<String, String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedCloudReport {
    pub uploaded_patches: Vec<String>,
    pub downloaded_patches: Vec<String>,
    pub uploaded_snapshots: Vec<String>,
    pub reconstructed_notes: Vec<String>,
    pub skipped_notes: usize,
}

pub struct ManagedCloudSyncEngine<'a, S: ManagedObjectStore> {
    book: &'a Book,
    store: S,
    provider: String,
}

impl<'a, S: ManagedObjectStore> ManagedCloudSyncEngine<'a, S> {
    pub fn new(
        book: &'a Book,
        store: S,
        provider: impl Into<String>,
    ) -> ManagedCloudSyncEngine<'a, S> {
        ManagedCloudSyncEngine {
            book,
            store,
            provider: provider.into(),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn into_store(self) -> S {
        self.store
    }

    pub fn sync(&mut self) -> CoreResult<ManagedCloudReport> {
        ensure_loro_enabled(&self.book.config.sync)?;
        let actor = actor_id_for(&self.book.root)?;
        let backend = select_crdt_backend(&self.book.config.sync);
        if backend.name() != crate::crdt::LORO_BACKEND {
            return Err(CoreError::Sync(
                "managed cloud sync requires a build with the Loro feature enabled".into(),
            ));
        }

        let mut report = ManagedCloudReport::default();
        let mut state = ManagedCloudSyncState::load(&self.book.root, &self.provider)?;

        self.reconstruct_remote_only_notes(&actor, &mut state, &mut report)?;
        self.reconcile_local_notes_to_sidecars(&actor)?;
        self.apply_remote_patches(&actor, &mut state, &mut report)?;
        self.upload_local_patches(&actor, &mut state, &mut report)?;
        self.upload_manifest(&state)?;

        state.save(&self.book.root)?;
        self.book.store.refresh()?;
        append_activity(
            &self.book.root,
            &SyncActivityEvent::new(
                "managed_cloud",
                "sync_complete",
                None,
                format!(
                    "{} uploaded, {} downloaded",
                    report.uploaded_patches.len(),
                    report.downloaded_patches.len()
                ),
            ),
        )?;
        Ok(report)
    }

    pub fn compact(&mut self) -> CoreResult<ManagedCloudReport> {
        ensure_loro_enabled(&self.book.config.sync)?;
        let actor = actor_id_for(&self.book.root)?;
        self.reconcile_local_notes_to_sidecars(&actor)?;
        let mut state = ManagedCloudSyncState::load(&self.book.root, &self.provider)?;
        let mut report = ManagedCloudReport::default();
        for note in self.book.store.read_all_notes()? {
            let ulid = note.id.ulid().to_string();
            let sidecar = layout::crdt_sidecar_path(&self.book.root, &note.id);
            if !sidecar.exists() {
                continue;
            }
            let snapshot_path = snapshot_path(self.book.metadata.book_id.as_str(), &ulid);
            let snapshot = std::fs::read(&sidecar)?;
            self.store.put(&snapshot_path, &snapshot)?;
            state.latest_snapshots.insert(ulid, snapshot_path.clone());
            report.uploaded_snapshots.push(snapshot_path);
        }
        self.upload_manifest(&state)?;
        state.save(&self.book.root)?;
        Ok(report)
    }

    pub fn load_manifest(&self) -> CoreResult<Option<BookManifest>> {
        let path = cloud_path(self.book.metadata.book_id.as_str(), MANIFEST_PATH);
        match self.store.get(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(CoreError::Sync(message)) if message.contains("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn reconstruct_remote_only_notes(
        &mut self,
        actor: &ActorId,
        state: &mut ManagedCloudSyncState,
        report: &mut ManagedCloudReport,
    ) -> CoreResult<()> {
        let Some(manifest) = self.load_manifest()? else {
            return Ok(());
        };
        let local_ulids = self
            .book
            .store
            .read_all_notes()?
            .into_iter()
            .map(|note| note.id.ulid().to_string())
            .collect::<BTreeSet<_>>();
        for record in manifest.notes {
            let ulid = record.note.id.ulid().to_string();
            if local_ulids.contains(&ulid) {
                continue;
            }
            let mut note = record.note.clone();
            note.body =
                self.remote_note_text(actor, &ulid, record.latest_snapshot_path.as_deref())?;
            self.book.save_note(&note)?;
            report
                .reconstructed_notes
                .push(note.id.as_str().to_string());
            if let Some(snapshot) = record.latest_snapshot_path {
                state.latest_snapshots.insert(ulid, snapshot);
            }
        }
        Ok(())
    }

    fn remote_note_text(
        &mut self,
        actor: &ActorId,
        ulid: &str,
        snapshot_path: Option<&str>,
    ) -> CoreResult<String> {
        let backend = select_crdt_backend(&self.book.config.sync);
        let mut doc = if let Some(path) = snapshot_path {
            backend.load_document(actor, &self.store.get(path)?)?
        } else {
            backend.new_document(actor, "")
        };
        for entry in self.patch_entries(ulid)? {
            doc.import_updates(&self.store.get(&entry.path)?)?;
        }
        Ok(doc.text())
    }

    fn reconcile_local_notes_to_sidecars(&self, actor: &ActorId) -> CoreResult<()> {
        let backend = select_crdt_backend(&self.book.config.sync);
        for note in self.book.store.read_all_notes()? {
            let sidecar = layout::crdt_sidecar_path(&self.book.root, &note.id);
            if let Some(parent) = sidecar.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut doc = if sidecar.exists() {
                backend.load_document(actor, &std::fs::read(&sidecar)?)?
            } else {
                backend.new_document(actor, &note.body)
            };
            if doc.text() != note.body {
                doc.set_text(&note.body);
            }
            std::fs::write(sidecar, doc.snapshot()?)?;
        }
        Ok(())
    }

    fn apply_remote_patches(
        &mut self,
        actor: &ActorId,
        state: &mut ManagedCloudSyncState,
        report: &mut ManagedCloudReport,
    ) -> CoreResult<()> {
        let backend = select_crdt_backend(&self.book.config.sync);
        for mut note in self.book.store.read_all_notes()? {
            let ulid = note.id.ulid().to_string();
            let sidecar = layout::crdt_sidecar_path(&self.book.root, &note.id);
            let mut doc = if sidecar.exists() {
                backend.load_document(actor, &std::fs::read(&sidecar)?)?
            } else {
                backend.new_document(actor, &note.body)
            };
            let mut changed = false;
            for entry in self.patch_entries(&ulid)? {
                if state.seen_patches.contains(&entry.path) {
                    continue;
                }
                doc.import_updates(&self.store.get(&entry.path)?)?;
                state.seen_patches.insert(entry.path.clone());
                report.downloaded_patches.push(entry.path);
                changed = true;
            }
            if changed {
                note.body = doc.text();
                self.book.save_note(&note)?;
                std::fs::write(sidecar, doc.snapshot()?)?;
            }
        }
        Ok(())
    }

    fn upload_local_patches(
        &mut self,
        actor: &ActorId,
        state: &mut ManagedCloudSyncState,
        report: &mut ManagedCloudReport,
    ) -> CoreResult<()> {
        let backend = select_crdt_backend(&self.book.config.sync);
        for note in self.book.store.read_all_notes()? {
            let ulid = note.id.ulid().to_string();
            let sidecar = layout::crdt_sidecar_path(&self.book.root, &note.id);
            let doc = if sidecar.exists() {
                backend.load_document(actor, &std::fs::read(&sidecar)?)?
            } else {
                backend.new_document(actor, &note.body)
            };
            let previous_vv = state
                .exported_version_vectors
                .get(&ulid)
                .map(String::as_str)
                .unwrap_or(EMPTY_VERSION_VECTOR_JSON);
            let updates = doc.updates_since_json(previous_vv)?;
            let current_vv = doc.version_vector_json()?;
            if content_revision(&updates) == content_revision(&[]) {
                state.exported_version_vectors.insert(ulid, current_vv);
                report.skipped_notes += 1;
                continue;
            }
            let path = patch_path(self.book.metadata.book_id.as_str(), &ulid, actor);
            self.store.put(&path, &updates)?;
            state.seen_patches.insert(path.clone());
            state.exported_version_vectors.insert(ulid, current_vv);
            report.uploaded_patches.push(path);
        }
        Ok(())
    }

    fn upload_manifest(&mut self, state: &ManagedCloudSyncState) -> CoreResult<()> {
        let notes = self
            .book
            .store
            .read_all_notes()?
            .into_iter()
            .map(|mut note| {
                let ulid = note.id.ulid().to_string();
                let note_path = layout::note_filename(&note.id);
                note.body.clear();
                CloudNoteRecord {
                    note,
                    note_path,
                    latest_snapshot_path: state.latest_snapshots.get(&ulid).cloned(),
                }
            })
            .collect();
        let manifest = BookManifest {
            schema_version: 1,
            book_id: self.book.metadata.book_id.clone(),
            name: self.book.metadata.name.clone(),
            updated_at: Utc::now(),
            notes,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        self.store.put(
            &cloud_path(self.book.metadata.book_id.as_str(), MANIFEST_PATH),
            &bytes,
        )
    }

    fn patch_entries(&self, ulid: &str) -> CoreResult<Vec<ManagedObjectEntry>> {
        let prefix = format!(
            "syllepsis-sync/books/{}/notes/{ulid}/patches/",
            self.book.metadata.book_id
        );
        let mut entries = self.store.list(&prefix)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }
}

impl ManagedCloudSyncState {
    pub fn load(book_root: &Path, provider: &str) -> CoreResult<ManagedCloudSyncState> {
        let path = state_path(book_root, provider);
        if !path.exists() {
            return Ok(ManagedCloudSyncState {
                provider: provider.to_string(),
                ..ManagedCloudSyncState::default()
            });
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn save(&self, book_root: &Path) -> CoreResult<()> {
        std::fs::create_dir_all(layout::sync_dir(book_root))?;
        std::fs::write(
            state_path(book_root, &self.provider),
            serde_json::to_vec_pretty(self)?,
        )?;
        Ok(())
    }
}

fn ensure_loro_enabled(cfg: &SyncConfig) -> CoreResult<()> {
    if cfg.crdt_backend != crate::crdt::LORO_BACKEND {
        return Err(CoreError::Sync(
            "managed cloud sync requires Loro; enable the Loro merge strategy in Sync settings"
                .into(),
        ));
    }
    Ok(())
}

fn state_path(book_root: &Path, provider: &str) -> PathBuf {
    layout::sync_dir(book_root).join(format!("{STATE_PREFIX}-{provider}.json"))
}

fn cloud_path(book_id: &str, path: &str) -> String {
    format!("syllepsis-sync/books/{book_id}/{path}")
}

fn patch_path(book_id: &str, ulid: &str, actor: &ActorId) -> String {
    format!(
        "syllepsis-sync/books/{book_id}/notes/{ulid}/patches/{}_{}.loro_patch",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        actor.as_str()
    )
}

fn snapshot_path(book_id: &str, ulid: &str) -> String {
    format!(
        "syllepsis-sync/books/{book_id}/notes/{ulid}/snapshots/{}.loro_snapshot",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    fn book_at(path: PathBuf) -> Book {
        Book::create(path, "Shared").unwrap()
    }

    #[cfg(feature = "loro")]
    #[test]
    fn two_devices_upload_concurrent_patches_and_converge() {
        let tmp = tempfile::tempdir().unwrap();
        let a = book_at(tmp.path().join("a"));
        let mut b = book_at(tmp.path().join("b"));
        b.metadata.book_id = a.metadata.book_id.clone();
        b.save_metadata().unwrap();
        let mut remote = MemoryManagedObjectStore::default();

        let mut note = a.new_note(ObjectType::Note, "shared").unwrap();
        note.body = "base.".into();
        a.save_note(&note).unwrap();
        remote = ManagedCloudSyncEngine::new(&a, remote, "memory")
            .into_store_after_sync()
            .unwrap();
        remote = ManagedCloudSyncEngine::new(&b, remote, "memory")
            .into_store_after_sync()
            .unwrap();

        let mut note_a = a.store.read_note(&note.id).unwrap();
        note_a.body = "base. from-a".into();
        a.save_note(&note_a).unwrap();
        let mut note_b = b.store.read_note(&note.id).unwrap();
        note_b.body = "from-b base.".into();
        b.save_note(&note_b).unwrap();

        remote = ManagedCloudSyncEngine::new(&a, remote, "memory")
            .into_store_after_sync()
            .unwrap();
        remote = ManagedCloudSyncEngine::new(&b, remote, "memory")
            .into_store_after_sync()
            .unwrap();
        remote = ManagedCloudSyncEngine::new(&a, remote, "memory")
            .into_store_after_sync()
            .unwrap();
        let _ = remote;

        a.store.refresh().unwrap();
        b.store.refresh().unwrap();
        let final_a = a.store.read_note(&note.id).unwrap().body;
        let final_b = b.store.read_note(&note.id).unwrap().body;
        assert_eq!(final_a, final_b);
        assert!(final_a.contains("from-a"));
        assert!(final_a.contains("from-b"));
    }

    #[cfg(feature = "loro")]
    #[test]
    fn duplicate_patch_download_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let a = book_at(tmp.path().join("a"));
        let mut b = book_at(tmp.path().join("b"));
        b.metadata.book_id = a.metadata.book_id.clone();
        b.save_metadata().unwrap();
        let mut remote = MemoryManagedObjectStore::default();
        let mut note = a.new_note(ObjectType::Note, "n").unwrap();
        note.body = "one".into();
        a.save_note(&note).unwrap();

        remote = ManagedCloudSyncEngine::new(&a, remote, "memory")
            .into_store_after_sync()
            .unwrap();
        let mut engine = ManagedCloudSyncEngine::new(&b, remote, "memory");
        let first = engine.sync().unwrap();
        let second = engine.sync().unwrap();
        assert!(!first.downloaded_patches.is_empty());
        assert!(second.downloaded_patches.is_empty());
    }

    #[test]
    fn managed_sync_refuses_lww() {
        let tmp = tempfile::tempdir().unwrap();
        let mut book = book_at(tmp.path().join("a"));
        book.config.sync.crdt_backend = crate::crdt::LWW_BACKEND.to_string();
        let mut engine =
            ManagedCloudSyncEngine::new(&book, MemoryManagedObjectStore::default(), "m");
        let err = engine.sync().unwrap_err();
        assert!(err.to_string().contains("requires Loro"));
    }

    impl<'a> ManagedCloudSyncEngine<'a, MemoryManagedObjectStore> {
        fn into_store_after_sync(mut self) -> CoreResult<MemoryManagedObjectStore> {
            self.sync()?;
            Ok(self.into_store())
        }
    }
}
