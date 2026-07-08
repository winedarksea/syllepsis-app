//! [`SyncEngine`]: one sync pass that reconciles a book's markdown with its CRDT sidecars and
//! exchanges both with a [`SyncProvider`].
//!
//! A pass is four steps:
//! 1. **Reconcile** — fold every note's markdown body (the local source of truth) into its CRDT
//!    sidecar, so an edit made in the editor *or* by an external tool is captured before diffing.
//! 2. **Fingerprint** — hash local files and list the remote, then run the pure
//!    [`plan`](super::plan::plan) to classify each path.
//! 3. **Apply** — push / pull / merge / conflict / delete per the plan. Note bodies merge through
//!    their sidecars (convergent); non-mergeable files get deterministic `.conflict-*` copies.
//! 4. **Persist** — write back the per-file [`SyncState`] so the next pass skips everything quiet,
//!    which is what stops write loops.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use chrono::Utc;

use crate::config::SyncConfig;
use crate::crdt::{select_crdt_backend, ActorId, CrdtBackend, LwwBackend, NoteCrdt};
use crate::error::CoreResult;
use crate::markdown::frontmatter;
use crate::model::Note;
use crate::storage::atomic::write_atomic;
use crate::sync::cloud_index::{is_cloud_index_path, IndexEntry};
use crate::sync::local_folder::content_revision;
use crate::sync::plan::{plan, SyncAction};
use crate::sync::provider::SyncProvider;
use crate::sync::state::SyncState;
use crate::sync::{is_embedding_sidecar, is_local_only, is_note_md, is_sidecar, sidecar_rel_path};

/// What one sync pass did. The vectors name the affected paths (for the UI's activity log and for
/// tests); [`is_noop`](SyncReport::is_noop) is the loop-prevention assertion — a second pass with
/// no real change must be a no-op.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncReport {
    pub pushed: Vec<String>,
    pub pulled: Vec<String>,
    pub merged: Vec<String>,
    pub conflicted: Vec<String>,
    pub deleted_local: Vec<String>,
    pub deleted_remote: Vec<String>,
    pub skipped: usize,
    /// Notes whose markdown failed to parse during sidecar reconciliation — their sidecar was
    /// left as-is (or absent) rather than failing the whole pass. The file itself still
    /// participates in the plan as opaque bytes (see [`local_fingerprints`]), so it is never
    /// mistaken for deleted.
    pub skipped_unparseable: Vec<String>,
    /// Sidecars that were corrupt and got rebuilt fresh from the markdown body (source of truth).
    pub rebuilt_sidecars: Vec<String>,
}

impl SyncReport {
    /// True if the pass changed nothing on either side (every path was skipped).
    pub fn is_noop(&self) -> bool {
        self.pushed.is_empty()
            && self.pulled.is_empty()
            && self.merged.is_empty()
            && self.conflicted.is_empty()
            && self.deleted_local.is_empty()
            && self.deleted_remote.is_empty()
    }
}

/// Drives a single book's sync against one provider. Constructed per pass; owns its provider and
/// CRDT backend so the call site need only hand it the book root and config.
pub struct SyncEngine {
    book_root: PathBuf,
    provider: Box<dyn SyncProvider>,
    backend: Box<dyn CrdtBackend>,
    actor: ActorId,
    conflict_marker: String,
    sync_crdt_sidecars: bool,
    sync_embedding_sidecars: bool,
    /// Book id stamped into the published cloud-index fragment (empty for the local-folder engine,
    /// whose `publish_index` is a no-op).
    book_id: String,
    /// Who owns this device's fragment; falls back to the actor id when no author is configured.
    author: String,
}

impl SyncEngine {
    /// Build an engine for `book_root` syncing through `provider`, selecting the CRDT backend and
    /// conflict-copy marker from `cfg` and attributing local edits to `actor`.
    pub fn new(
        book_root: impl Into<PathBuf>,
        provider: Box<dyn SyncProvider>,
        actor: ActorId,
        cfg: &SyncConfig,
    ) -> SyncEngine {
        let author = actor.as_str().to_string();
        SyncEngine {
            book_root: book_root.into(),
            provider,
            backend: select_crdt_backend(cfg),
            actor,
            conflict_marker: cfg.conflict_marker.clone(),
            sync_crdt_sidecars: true,
            sync_embedding_sidecars: true,
            book_id: String::new(),
            author,
        }
    }

    /// Build an engine for remotes intended to stay human-readable. Local CRDT and embedding
    /// sidecars remain local implementation details; markdown and other user-authored files sync.
    /// `book_id` and `author` are stamped into the cloud-index fragment this device publishes; an
    /// empty `author` falls back to the device actor.
    pub fn new_human_readable_remote(
        book_root: impl Into<PathBuf>,
        provider: Box<dyn SyncProvider>,
        actor: ActorId,
        cfg: &SyncConfig,
        book_id: impl Into<String>,
        author: impl Into<String>,
    ) -> SyncEngine {
        let mut engine = SyncEngine::new(book_root, provider, actor, cfg);
        engine.sync_crdt_sidecars = false;
        engine.sync_embedding_sidecars = false;
        engine.book_id = book_id.into();
        let author = author.into();
        if !author.trim().is_empty() {
            engine.author = author;
        }
        engine
    }

    /// Run one sync pass and report what changed.
    pub fn sync(&self) -> CoreResult<SyncReport> {
        let mut state = SyncState::load(&self.book_root, self.provider.name())?;
        let mut report = SyncReport::default();

        // 1. Markdown → sidecar, so local/external edits are in the CRDT before we diff.
        self.reconcile_sidecars(&mut report)?;

        // 2. Fingerprint both sides. Sidecars and device-local dirs are excluded from the planned
        //    set — sidecars ride along with their note, never planned independently.
        let local = self.local_fingerprints()?;
        let mut remote_all = BTreeMap::new();
        for entry in self.provider.list()? {
            remote_all.insert(entry.path, entry.revision);
        }
        let remote_primary: BTreeMap<String, String> = remote_all
            .iter()
            .filter(|(p, _)| self.is_syncable_primary_path(p))
            .map(|(p, r)| (p.clone(), r.clone()))
            .collect();

        // 3. Apply the plan.
        for action in plan(&local, &remote_primary, &state, is_note_md) {
            self.apply(
                action,
                &local,
                &remote_primary,
                &remote_all,
                &mut state,
                &mut report,
            )?;
        }

        // 4. Forget state for primary files now absent on both sides (deleted everywhere); sidecar
        //    state is pruned alongside its note in the delete handlers.
        let stale: Vec<String> = state
            .files
            .keys()
            .filter(|p| !is_sidecar(p) && !local.contains_key(*p) && !remote_all.contains_key(*p))
            .cloned()
            .collect();
        for path in stale {
            state.forget(&path);
        }
        state.save(&self.book_root)?;

        self.publish_index(&state, &report)?;
        Ok(report)
    }

    /// Publish this device's cloud-index fragment from the just-persisted [`SyncState`]: one entry
    /// per live syncable primary file (its remote revision) plus a tombstone for each path deleted
    /// remotely this pass. Written once per pass (not per put) so the fragment is uploaded at most
    /// once. A no-op for providers without a cheap index (the default `publish_index`).
    fn publish_index(&self, state: &SyncState, report: &SyncReport) -> CoreResult<()> {
        let now = Utc::now();
        let mut entries: BTreeMap<String, IndexEntry> = BTreeMap::new();
        for (path, synced) in &state.files {
            if !self.is_syncable_primary_path(path) {
                continue;
            }
            entries.insert(
                path.clone(),
                IndexEntry {
                    revision: synced.remote_revision.clone(),
                    updated_at: now,
                    deleted: false,
                },
            );
        }
        for path in &report.deleted_remote {
            entries.insert(
                path.clone(),
                IndexEntry {
                    revision: String::new(),
                    updated_at: now,
                    deleted: true,
                },
            );
        }
        self.provider
            .publish_index(self.actor.as_str(), &self.author, &self.book_id, &entries)
    }

    fn apply(
        &self,
        action: SyncAction,
        local: &BTreeMap<String, String>,
        remote_primary: &BTreeMap<String, String>,
        remote_all: &BTreeMap<String, String>,
        state: &mut SyncState,
        report: &mut SyncReport,
    ) -> CoreResult<()> {
        match action {
            SyncAction::Push(path) => {
                self.push_file(&path, state)?;
                self.push_sidecar_of(&path, state)?;
                report.pushed.push(path);
            }
            SyncAction::Pull(path) => {
                let rev = remote_primary.get(&path).cloned().unwrap_or_default();
                self.pull_file(&path, &rev, state)?;
                self.pull_sidecar_of(&path, remote_all, state)?;
                report.pulled.push(path);
            }
            SyncAction::Merge(path) => {
                if self.merge_note(&path, remote_all, state, report)? {
                    report.merged.push(path);
                } else {
                    report.conflicted.push(path);
                }
            }
            SyncAction::Conflict(path) => {
                if is_embedding_sidecar(&path) {
                    self.resolve_embedding_conflict(&path, state)?;
                } else {
                    self.resolve_conflict(&path, state)?;
                }
                report.conflicted.push(path);
            }
            SyncAction::DeleteLocal(path) => {
                let _ = std::fs::remove_file(self.full(&path));
                self.drop_sidecar(&path, state, true)?;
                state.forget(&path);
                report.deleted_local.push(path);
            }
            SyncAction::DeleteRemote(path) => {
                self.provider.delete(&path)?;
                self.drop_sidecar(&path, state, true)?;
                state.forget(&path);
                report.deleted_remote.push(path);
            }
            SyncAction::Skip(path) => {
                // Record state for an as-yet-untracked but already-matching file so future passes
                // recognize it as quiet.
                if let (Some(lh), Some(rr)) = (local.get(&path), remote_primary.get(&path)) {
                    state.mark_synced(&path, lh.clone(), rr.clone());
                }
                report.skipped += 1;
            }
        }
        Ok(())
    }

    fn is_syncable_primary_path(&self, path: &str) -> bool {
        if is_local_only(path) || is_sidecar(path) || is_cloud_index_path(path) {
            return false;
        }
        self.sync_embedding_sidecars || !is_embedding_sidecar(path)
    }

    /// The always-available LWW backend, used in place of the book's configured backend for
    /// PIN-locked notes (see [`Self::doc_backend_for`]). A `const` unit value so it can be handed
    /// out as a `'static` reference without allocating.
    const LOCKED_NOTE_BACKEND: LwwBackend = LwwBackend;

    /// The CRDT backend to use for `note`'s sidecar. PIN-locked notes always use the whole-body LWW
    /// register regardless of the book's configured backend: fine-grained merge (Loro) over
    /// ciphertext is meaningless (any concurrent byte-level merge just produces undecryptable
    /// garbage), and LWW is compiled into every build so a locked note's sidecar never depends on
    /// an optional feature. If the note file itself conflicts, [`Self::merge_note`] resolves it as
    /// a whole-file conflict instead of merging only the body, because the ciphertext body and
    /// nonce-bearing frontmatter must stay paired.
    fn doc_backend_for(&self, note: &Note) -> &dyn CrdtBackend {
        if note.is_pin_locked() {
            &Self::LOCKED_NOTE_BACKEND
        } else {
            self.backend.as_ref()
        }
    }

    /// Step 1: ensure every local note has a sidecar whose CRDT text equals its markdown body.
    /// A note that fails to parse (bad frontmatter) or a sidecar that fails to load (corrupt
    /// `.crdt` file) must never fail the whole sync pass — the file stays exactly as it is on
    /// disk (see [`local_fingerprints`]'s invariant) and the issue is reported instead.
    fn reconcile_sidecars(&self, report: &mut SyncReport) -> CoreResult<()> {
        for rel in self.walk_files()? {
            if !is_note_md(&rel) {
                continue;
            }
            let note = match frontmatter::parse_note(&std::fs::read_to_string(self.full(&rel))?) {
                Ok(note) => note,
                Err(_) => {
                    report.skipped_unparseable.push(rel);
                    continue;
                }
            };
            let sidecar_rel = match sidecar_rel_path(&rel) {
                Some(s) => s,
                None => continue,
            };
            let sidecar_full = self.full(&sidecar_rel);
            let backend = self.doc_backend_for(&note);
            if sidecar_full.exists() {
                let (mut doc, corrupt) = match backend
                    .load_document(&self.actor, &std::fs::read(&sidecar_full)?)
                {
                    Ok(doc) => (doc, false),
                    Err(_) => {
                        // Corrupt sidecar: markdown is the source of truth, rebuild from it. Also
                        // the migration path for a pre-existing Loro sidecar on a note that has
                        // since been locked: it fails to load under LWW and rebuilds here.
                        report.rebuilt_sidecars.push(rel.clone());
                        (backend.new_document(&self.actor, &note.body), true)
                    }
                };
                if corrupt || doc.text() != note.body {
                    doc.set_text(&note.body); // captures the local/external edit (idempotent if equal)
                    self.write_sidecar(&sidecar_full, doc.as_ref())?;
                }
            } else {
                let doc = backend.new_document(&self.actor, &note.body);
                self.write_sidecar(&sidecar_full, doc.as_ref())?;
            }
        }
        Ok(())
    }

    /// Both sides changed a note: merge the two CRDT sidecars, render the merged body back into the
    /// markdown, and push both. The body converges; markdown frontmatter takes the local copy (a
    /// known POC limitation — concurrent *metadata* edits last-writer-win, concurrent *text* edits
    /// merge).
    ///
    /// Returns `false` when there is no reliable body to merge against — the local note failed to
    /// parse, the remote note is not valid UTF-8, or the remote note fails to parse — so this falls
    /// back to a byte-level [`resolve_conflict`](Self::resolve_conflict) (both variants preserved)
    /// instead of propagating an error that would fail the whole pass. The caller should report the
    /// path as conflicted rather than merged in that case. A corrupt *local* sidecar is not such a
    /// case: markdown is the source of truth, so it is silently rebuilt from the local note body
    /// (recorded in `report.rebuilt_sidecars`) and the merge proceeds normally.
    fn merge_note(
        &self,
        path: &str,
        remote_all: &BTreeMap<String, String>,
        state: &mut SyncState,
        report: &mut SyncReport,
    ) -> CoreResult<bool> {
        let sidecar_rel = sidecar_rel_path(path).expect("merge target is a note");
        let sidecar_full = self.full(&sidecar_rel);
        let mut note = match frontmatter::parse_note(&std::fs::read_to_string(self.full(path))?) {
            Ok(note) => note,
            Err(_) => {
                self.resolve_conflict(path, state)?;
                return Ok(false);
            }
        };
        if note.is_pin_locked() {
            self.resolve_conflict(path, state)?;
            return Ok(false);
        }
        let remote_bytes = self.provider.get(path)?;
        let Ok(remote_text) = String::from_utf8(remote_bytes) else {
            self.resolve_conflict(path, state)?;
            return Ok(false);
        };
        let Ok(remote_note) = frontmatter::parse_note(&remote_text) else {
            self.resolve_conflict(path, state)?;
            return Ok(false);
        };
        if remote_note.is_pin_locked() {
            self.resolve_conflict(path, state)?;
            return Ok(false);
        }

        let backend = self.doc_backend_for(&note);
        let mut doc = if sidecar_full.exists() {
            match backend.load_document(&self.actor, &std::fs::read(&sidecar_full)?) {
                Ok(doc) => doc,
                Err(_) => {
                    // Corrupt local sidecar: markdown is the source of truth, rebuild from it. Also
                    // the migration path for a pre-existing Loro sidecar on a note that has since
                    // been locked: it fails to load under LWW and rebuilds here.
                    report.rebuilt_sidecars.push(path.to_string());
                    backend.new_document(&self.actor, &note.body)
                }
            }
        } else {
            backend.new_document(&self.actor, &note.body)
        };

        let merged = if remote_all.contains_key(&sidecar_rel) {
            match doc.merge(&self.provider.get(&sidecar_rel)?) {
                Ok(()) => true,
                // Corrupt remote sidecar: fall back to the note-body path below, as if the remote
                // simply had no sidecar.
                Err(_) => self.fold_remote_note_body(&mut doc, path)?,
            }
        } else {
            self.fold_remote_note_body(&mut doc, path)?
        };
        if !merged {
            self.resolve_conflict(path, state)?;
            return Ok(false);
        }

        note.body = doc.text();
        self.write_sidecar(&sidecar_full, doc.as_ref())?;
        let serialized = frontmatter::serialize_note(&note)?;
        write_atomic(&self.full(path), serialized.as_bytes())?;

        let note_rev = self.provider.put(path, serialized.as_bytes())?;
        state.mark_synced(path, content_revision(serialized.as_bytes()), note_rev);
        self.push_sidecar_of(path, state)?;
        Ok(true)
    }

    /// Remote changed the note but has no usable sidecar (edited by a non-CRDT tool, or the
    /// remote sidecar was corrupt): fold the remote body into `doc` as a local edit so it still
    /// participates in convergence. Requires the remote bytes to be valid UTF-8 and parseable —
    /// on either failure, returns `false` so the caller falls back to a byte-level conflict rather
    /// than writing lossy (`U+FFFD`) replacement text back to both sides.
    fn fold_remote_note_body(&self, doc: &mut Box<dyn NoteCrdt>, path: &str) -> CoreResult<bool> {
        let remote_bytes = self.provider.get(path)?;
        let Ok(remote_text) = String::from_utf8(remote_bytes) else {
            return Ok(false);
        };
        let Ok(remote_note) = frontmatter::parse_note(&remote_text) else {
            return Ok(false);
        };
        doc.set_text(&remote_note.body);
        Ok(true)
    }

    /// Both sides changed a non-mergeable file: pick a deterministic winner (greater content hash),
    /// keep it live everywhere, and preserve the loser as a `.conflict-{hash}` copy. Because the
    /// winner is a pure function of the two contents, both devices converge on the same pair.
    fn resolve_conflict(&self, path: &str, state: &mut SyncState) -> CoreResult<()> {
        let local_bytes = std::fs::read(self.full(path))?;
        let remote_bytes = self.provider.get(path)?;
        let local_hash = content_revision(&local_bytes);
        let remote_hash = content_revision(&remote_bytes);

        let (winner, loser, loser_hash) = if local_hash >= remote_hash {
            (local_bytes, remote_bytes, remote_hash)
        } else {
            (remote_bytes, local_bytes, local_hash)
        };
        let conflict_rel = conflict_path(path, &self.conflict_marker, &loser_hash);

        write_atomic(&self.full(path), &winner)?;
        self.write_local(&conflict_rel, &loser)?;

        let win_rev = self.provider.put(path, &winner)?;
        state.mark_synced(path, content_revision(&winner), win_rev);
        let conflict_rev = self.provider.put(&conflict_rel, &loser)?;
        state.mark_synced(&conflict_rel, content_revision(&loser), conflict_rev);
        Ok(())
    }

    fn resolve_embedding_conflict(&self, path: &str, state: &mut SyncState) -> CoreResult<()> {
        let local_bytes = std::fs::read(self.full(path))?;
        let remote_bytes = self.provider.get(path)?;
        let book = crate::storage::Book::open(&self.book_root)?;
        let local_rank = crate::embeddings::sidecar_preference_rank(&book, &local_bytes);
        let remote_rank = crate::embeddings::sidecar_preference_rank(&book, &remote_bytes);
        let winner = if local_rank >= remote_rank {
            local_bytes
        } else {
            remote_bytes
        };
        self.write_local(path, &winner)?;
        let revision = self.provider.put(path, &winner)?;
        state.mark_synced(path, content_revision(&winner), revision);
        Ok(())
    }

    // --- file/sidecar primitives ---------------------------------------------------------------

    fn push_file(&self, rel: &str, state: &mut SyncState) -> CoreResult<()> {
        let bytes = std::fs::read(self.full(rel))?;
        let rev = self.provider.put(rel, &bytes)?;
        state.mark_synced(rel, content_revision(&bytes), rev);
        Ok(())
    }

    fn pull_file(&self, rel: &str, remote_rev: &str, state: &mut SyncState) -> CoreResult<()> {
        let bytes = self.provider.get(rel)?;
        self.write_local(rel, &bytes)?;
        state.mark_synced(rel, content_revision(&bytes), remote_rev);
        Ok(())
    }

    fn push_sidecar_of(&self, note_path: &str, state: &mut SyncState) -> CoreResult<()> {
        if !self.sync_crdt_sidecars {
            return Ok(());
        }
        if let Some(sidecar) = sidecar_rel_path(note_path) {
            if self.full(&sidecar).exists() {
                self.push_file(&sidecar, state)?;
            }
        }
        Ok(())
    }

    fn pull_sidecar_of(
        &self,
        note_path: &str,
        remote_all: &BTreeMap<String, String>,
        state: &mut SyncState,
    ) -> CoreResult<()> {
        if !self.sync_crdt_sidecars {
            return Ok(());
        }
        if let Some(sidecar) = sidecar_rel_path(note_path) {
            if let Some(rev) = remote_all.get(&sidecar) {
                self.pull_file(&sidecar, rev, state)?;
            }
        }
        Ok(())
    }

    /// Remove a note's sidecar locally and (when `from_remote`) on the remote, forgetting its state.
    fn drop_sidecar(
        &self,
        note_path: &str,
        state: &mut SyncState,
        from_remote: bool,
    ) -> CoreResult<()> {
        if !is_note_md(note_path) {
            return Ok(());
        }
        if let Some(sidecar) = sidecar_rel_path(note_path) {
            let _ = std::fs::remove_file(self.full(&sidecar));
            if from_remote {
                self.provider.delete(&sidecar)?;
            }
            state.forget(&sidecar);
        }
        Ok(())
    }

    fn write_sidecar(&self, full: &Path, doc: &dyn NoteCrdt) -> CoreResult<()> {
        write_atomic(full, &doc.snapshot()?)
    }

    fn write_local(&self, rel: &str, bytes: &[u8]) -> CoreResult<()> {
        write_atomic(&self.full(rel), bytes)
    }

    /// All syncable local files: every file under the book except device-local bookkeeping
    /// (`_sync/`), ephemeral caches (`_derived/`), and the CRDT sidecars (`_crdt/`, handled with
    /// their notes), each mapped to its content hash.
    ///
    /// **Invariant**: this must include *every* on-disk file that isn't excluded above,
    /// parseable or not — hashed as raw bytes, never opened/parsed. `plan()` treats a path's
    /// absence here as "deleted locally", so an unparseable note dropping out of this map would
    /// make sync delete it on the remote. A corrupt note must still show up as present, just
    /// unmergeable ([`merge_note`](Self::merge_note) and [`reconcile_sidecars`](Self::reconcile_sidecars)
    /// handle that by falling back to conflict resolution or skipping, never by omission).
    fn local_fingerprints(&self) -> CoreResult<BTreeMap<String, String>> {
        let mut map = BTreeMap::new();
        for rel in self.walk_files()? {
            if is_local_only(&rel) || is_sidecar(&rel) {
                continue;
            }
            if !self.sync_embedding_sidecars && is_embedding_sidecar(&rel) {
                continue;
            }
            map.insert(
                rel.clone(),
                content_revision(&std::fs::read(self.full(&rel))?),
            );
        }
        Ok(map)
    }

    /// Every file under the book root as a book-relative POSIX path.
    fn walk_files(&self) -> CoreResult<Vec<String>> {
        let mut out = Vec::new();
        collect(&self.book_root, &self.book_root, &mut out)?;
        Ok(out)
    }

    fn full(&self, rel: &str) -> PathBuf {
        let mut path = self.book_root.clone();
        for segment in rel.split('/').filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }
}

/// Build the conflict-copy path for `path`: `{stem}.{marker}-{hash8}.{ext}` next to the original.
/// The hash makes the name deterministic across devices, so both write the identical copy.
fn conflict_path(path: &str, marker: &str, loser_hash: &str) -> String {
    let short = &loser_hash[..loser_hash.len().min(8)];
    let p = Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let name = match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}.{marker}-{short}.{ext}"),
        None => format!("{stem}.{marker}-{short}"),
    };
    match p
        .parent()
        .and_then(|parent| parent.to_str())
        .filter(|s| !s.is_empty())
    {
        Some(parent) => format!("{parent}/{name}"),
        None => name,
    }
}

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
    use crate::model::ObjectType;
    use crate::storage::{Book, NoteStore};
    use crate::sync::cloud_index::IndexedLocalFolderSync;
    use crate::sync::{actor_id_for, LocalFolderSync};
    use std::sync::atomic::Ordering::Relaxed;

    /// One device: a book dir plus a freshly-built engine sharing the given remote folder.
    struct Device {
        book: Book,
        remote: PathBuf,
        cfg: SyncConfig,
    }

    impl Device {
        fn engine(&self) -> SyncEngine {
            let provider = Box::new(LocalFolderSync::open(&self.remote).unwrap());
            let actor = actor_id_for(self.book.root.as_path()).unwrap();
            SyncEngine::new(self.book.root.clone(), provider, actor, &self.cfg)
        }
        fn human_readable_engine(&self) -> SyncEngine {
            let provider = Box::new(LocalFolderSync::open(&self.remote).unwrap());
            let actor = actor_id_for(self.book.root.as_path()).unwrap();
            SyncEngine::new_human_readable_remote(
                self.book.root.clone(),
                provider,
                actor,
                &self.cfg,
                self.book.metadata.book_id.clone(),
                "",
            )
        }
        /// A human-readable engine over the fragment-aware [`IndexedLocalFolderSync`] test double,
        /// returned alongside a handle to its content-fetch counter (the bandwidth metric).
        fn indexed_engine(&self) -> (SyncEngine, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
            let provider = IndexedLocalFolderSync::open(&self.remote).unwrap();
            let counter = provider.get_count_handle();
            let actor = actor_id_for(self.book.root.as_path()).unwrap();
            let engine = SyncEngine::new_human_readable_remote(
                self.book.root.clone(),
                Box::new(provider),
                actor,
                &self.cfg,
                self.book.metadata.book_id.clone(),
                "",
            );
            (engine, counter)
        }
        fn sync(&self) -> SyncReport {
            self.engine().sync().unwrap()
        }
    }

    fn two_devices() -> (tempfile::TempDir, Device, Device) {
        two_devices_with(SyncConfig {
            crdt_backend: crate::crdt::LWW_BACKEND.to_string(),
            ..SyncConfig::default()
        })
    }

    fn two_devices_with(cfg: SyncConfig) -> (tempfile::TempDir, Device, Device) {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let a = Book::create(tmp.path().join("device-a"), "Shared").unwrap();
        let mut b = Book::create(tmp.path().join("device-b"), "Shared").unwrap();
        b.metadata.book_id = a.metadata.book_id.clone();
        b.save_metadata().unwrap();
        (
            tmp,
            Device {
                book: a,
                remote: remote.clone(),
                cfg: cfg.clone(),
            },
            Device {
                book: b,
                remote,
                cfg,
            },
        )
    }

    fn body_of(book: &Book, id: &crate::id::NoteId) -> String {
        book.store.read_note(id).unwrap().body
    }

    #[test]
    fn note_created_on_one_device_pulls_to_the_other() {
        let (_tmp, a, b) = two_devices();
        let note = a.book.new_note(ObjectType::Note, "kitchen").unwrap();
        a.book
            .save_note(&{
                let mut n = note.clone();
                n.body = "breaker panel notes".into();
                n
            })
            .unwrap();

        let push = a.sync();
        assert!(push.pushed.iter().any(|p| p.ends_with(".md")));

        let pull = b.sync();
        assert!(pull.pulled.iter().any(|p| p.ends_with(".md")));
        // The note (and its sidecar) arrived on device B.
        b.book.store.refresh().unwrap();
        assert_eq!(body_of(&b.book, &note.id), "breaker panel notes");
        assert!(
            crate::storage::layout::crdt_sidecar_path(b.book.root.as_path(), &note.id).exists()
        );
    }

    #[test]
    fn second_sync_with_no_changes_is_a_noop() {
        // The core loop-prevention guarantee.
        let (_tmp, a, b) = two_devices();
        let note = a.book.new_note(ObjectType::Note, "n").unwrap();
        let mut n = note.clone();
        n.body = "content".into();
        a.book.save_note(&n).unwrap();

        a.sync();
        b.sync();
        // Everything is now in sync; a second pass on each side must touch nothing.
        assert!(a.sync().is_noop(), "device A re-sync should be a no-op");
        assert!(b.sync().is_noop(), "device B re-sync should be a no-op");
    }

    #[test]
    fn embedding_sidecars_sync_without_entering_the_note_scan() {
        let (_tmp, a, b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "embedded").unwrap();
        note.body = "garden compost".into();
        a.book.save_note(&note).unwrap();
        crate::embeddings::repository::write_test_sidecars(&a.book, &[note.clone()]);

        a.sync();
        b.sync();

        let sidecar = crate::storage::layout::embedding_sidecar_path(&b.book.root, &note.id);
        assert!(sidecar.exists());
        assert!(crate::embeddings::read_sidecar(&sidecar).is_ok());
        b.book.store.refresh().unwrap();
        assert_eq!(b.book.store.read_all_notes().unwrap().len(), 1);
    }

    #[test]
    fn human_readable_remote_does_not_publish_sidecar_files() {
        let (_tmp, a, _b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "readable").unwrap();
        note.body = "visible markdown".into();
        a.book.save_note(&note).unwrap();
        crate::embeddings::repository::write_test_sidecars(&a.book, &[note.clone()]);

        let report = a.human_readable_engine().sync().unwrap();

        assert!(report.pushed.iter().any(|path| path.ends_with(".md")));
        assert!(a.remote.join(format!("{}.md", note.id.as_str())).exists());
        assert!(!a.remote.join("_crdt").exists());
        assert!(!a.remote.join("_embeddings").exists());
    }

    fn add_note(device: &Device, title: &str, body: &str) -> crate::id::NoteId {
        let mut note = device.book.new_note(ObjectType::Note, title).unwrap();
        note.body = body.into();
        device.book.save_note(&note).unwrap();
        note.id
    }

    #[test]
    fn indexed_round_trip_pulls_only_what_changed() {
        // A publishes a note + its fragment; B learns the note's revision from the fragment and
        // fetches only the file it must pull — not every remote file just to fingerprint it.
        let (_tmp, a, b) = two_devices();
        let id = add_note(&a, "kitchen", "breaker panel notes");

        let (a_engine, a_count) = a.indexed_engine();
        a_engine.sync().unwrap();
        assert_eq!(a_count.load(Relaxed), 0, "pushing device fetches nothing");

        let (b_engine, b_count) = b.indexed_engine();
        let report = b_engine.sync().unwrap();

        assert!(report.pulled.iter().any(|p| p.ends_with(".md")));
        // Only the pulled note is fetched; the identical _book.md is skipped via the index, never
        // downloaded to fingerprint it.
        assert_eq!(b_count.load(Relaxed), 1, "only the Pull target is fetched");
        b.book.store.refresh().unwrap();
        assert_eq!(body_of(&b.book, &id), "breaker panel notes");
        // The index fragment is the only non-markdown thing on the remote.
        assert!(b.remote.join(crate::sync::CLOUD_INDEX_DIR).exists());
        assert!(!b.remote.join("_crdt").exists());
    }

    #[test]
    fn indexed_quiet_second_pass_fetches_nothing() {
        // The bandwidth win: once both devices are in sync, a subsequent pass downloads no bytes.
        let (_tmp, a, b) = two_devices();
        add_note(&a, "n", "content");
        a.indexed_engine().0.sync().unwrap();
        b.indexed_engine().0.sync().unwrap();

        let (b_engine, b_count) = b.indexed_engine();
        let report = b_engine.sync().unwrap();
        assert!(report.is_noop(), "quiet pass changes nothing");
        assert_eq!(
            b_count.load(Relaxed),
            0,
            "quiet pass fetches no file bodies"
        );
    }

    #[test]
    fn indexed_fragments_from_two_devices_merge_for_a_third_reader() {
        // A and B each create a different note and publish their own fragment (no overwrite race).
        // A third reader sees both notes' revisions from the merged index without hashing them.
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote");
        let a_book = Book::create(tmp.path().join("device-a"), "Shared").unwrap();
        let book_id = a_book.metadata.book_id.clone();
        let mut b_book = Book::create(tmp.path().join("device-b"), "Shared").unwrap();
        b_book.metadata.book_id = book_id.clone();
        b_book.save_metadata().unwrap();
        let mut c_book = Book::create(tmp.path().join("device-c"), "Shared").unwrap();
        c_book.metadata.book_id = book_id;
        c_book.save_metadata().unwrap();
        let cfg = SyncConfig {
            crdt_backend: crate::crdt::LWW_BACKEND.to_string(),
            ..SyncConfig::default()
        };
        let a = Device {
            book: a_book,
            remote: remote.clone(),
            cfg: cfg.clone(),
        };
        let b = Device {
            book: b_book,
            remote: remote.clone(),
            cfg: cfg.clone(),
        };
        let c = Device {
            book: c_book,
            remote,
            cfg,
        };

        let id_a = add_note(&a, "from-a", "alpha body");
        let id_b = add_note(&b, "from-b", "beta body");
        a.indexed_engine().0.sync().unwrap();
        b.indexed_engine().0.sync().unwrap();

        // Two independent fragments exist; neither clobbered the other.
        let fragments = std::fs::read_dir(a.remote.join(crate::sync::CLOUD_INDEX_DIR))
            .unwrap()
            .count();
        assert_eq!(fragments, 2, "each device wrote its own fragment");

        let (c_engine, c_count) = c.indexed_engine();
        c_engine.sync().unwrap();
        c.book.store.refresh().unwrap();
        assert_eq!(body_of(&c.book, &id_a), "alpha body");
        assert_eq!(body_of(&c.book, &id_b), "beta body");
        // Two notes pulled; nothing hashed via the fallback (revisions all came from the index).
        assert_eq!(
            c_count.load(Relaxed),
            2,
            "only the two pulled notes are fetched"
        );
    }

    #[test]
    fn indexed_tombstone_delete_propagates() {
        let (_tmp, a, b) = two_devices();
        let id = add_note(&a, "doomed", "to be deleted");
        a.indexed_engine().0.sync().unwrap();
        b.indexed_engine().0.sync().unwrap();
        b.book.store.refresh().unwrap();
        assert!(b.book.store.read_note(&id).is_ok());

        // A deletes the note and syncs: the remote file is removed and a tombstone is published.
        a.book.delete_note(&id).unwrap();
        let del = a.indexed_engine().0.sync().unwrap();
        assert!(del.deleted_remote.iter().any(|p| p.ends_with(".md")));

        let report = b.indexed_engine().0.sync().unwrap();
        assert!(
            report.deleted_local.iter().any(|p| p.ends_with(".md")),
            "the delete propagates to device B"
        );
    }

    #[test]
    fn indexed_pre_index_books_fall_back_then_go_cheap() {
        // Remote files written by a pre-index client (no fragments): the first list() hashes them,
        // then this device publishes a fragment, so the next pass is cheap.
        let (_tmp, a, b) = two_devices();
        add_note(&a, "legacy", "pre-existing");
        // Push with the plain folder provider so no fragment is written (simulates an old client).
        a.human_readable_engine().sync().unwrap();
        assert!(!a.remote.join(crate::sync::CLOUD_INDEX_DIR).exists());

        let (b_engine, b_count) = b.indexed_engine();
        b_engine.sync().unwrap();
        // First pass had to hash the unindexed remote files (fallback fired at least once).
        assert!(
            b_count.load(Relaxed) >= 1,
            "first pass hashes unindexed files"
        );

        let (b_engine2, b_count2) = b.indexed_engine();
        assert!(b_engine2.sync().unwrap().is_noop());
        assert_eq!(
            b_count2.load(Relaxed),
            0,
            "second pass is cheap once a fragment exists"
        );
    }

    #[test]
    fn os_metadata_files_are_not_synced() {
        let (_tmp, a, _b) = two_devices();
        std::fs::write(a.book.root.join(".DS_Store"), b"finder").unwrap();
        std::fs::create_dir_all(a.book.root.join("_categories")).unwrap();
        std::fs::write(a.book.root.join("_categories/.DS_Store"), b"finder").unwrap();

        let report = a.sync();

        assert!(!report.pushed.iter().any(|path| path.contains(".DS_Store")));
        assert!(!a.remote.join(".DS_Store").exists());
        assert!(!a.remote.join("_categories/.DS_Store").exists());
    }

    #[test]
    fn concurrent_note_edits_converge() {
        let (_tmp, a, b) = two_devices();
        let note = a.book.new_note(ObjectType::Note, "shared").unwrap();
        let mut base = note.clone();
        base.body = "base".into();
        a.book.save_note(&base).unwrap();
        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        // Both devices edit the same note's body before the next sync.
        let mut ea = a.book.store.read_note(&note.id).unwrap();
        ea.body = "edit from A".into();
        a.book.save_note(&ea).unwrap();
        let mut eb = b.book.store.read_note(&note.id).unwrap();
        eb.body = "edit from B".into();
        b.book.save_note(&eb).unwrap();

        // Sync both ways until quiescent; replicas must converge to the same body.
        for _ in 0..3 {
            a.sync();
            b.sync();
        }
        a.book.store.refresh().unwrap();
        b.book.store.refresh().unwrap();
        let final_a = body_of(&a.book, &note.id);
        let final_b = body_of(&b.book, &note.id);
        assert_eq!(final_a, final_b, "replicas must converge");
        assert!(
            final_a == "edit from A" || final_a == "edit from B",
            "LWW keeps one of the two concurrent edits, got {final_a:?}"
        );
        assert!(a.sync().is_noop() && b.sync().is_noop(), "must settle");
    }

    #[test]
    fn concurrent_category_edits_produce_a_conflict_copy() {
        let (_tmp, a, b) = two_devices();
        // A non-note file (a category) edited on both devices is not CRDT-mergeable.
        crate::app::commands::create_category(&a.book, crate::model::Category::new("electrical"))
            .unwrap();
        a.sync();
        b.sync();

        let mut cat_a = crate::model::Category::new("electrical");
        cat_a.long_name = "From A".into();
        crate::app::commands::create_category(&a.book, cat_a).unwrap();
        let mut cat_b = crate::model::Category::new("electrical");
        cat_b.long_name = "From B".into();
        crate::app::commands::create_category(&b.book, cat_b).unwrap();

        for _ in 0..3 {
            a.sync();
            b.sync();
        }
        // A deterministic conflict copy of the category exists on both devices and they converge.
        let has_conflict = |root: &Path| {
            std::fs::read_dir(root.join("_categories"))
                .unwrap()
                .filter_map(|e| e.ok())
                .any(|e| e.file_name().to_string_lossy().contains("conflict"))
        };
        assert!(has_conflict(a.book.root.as_path()));
        assert!(has_conflict(b.book.root.as_path()));
        assert!(a.sync().is_noop() && b.sync().is_noop());
    }

    #[cfg(feature = "loro")]
    #[test]
    fn loro_backend_merges_both_concurrent_edits_through_the_engine() {
        // With the fine-grained backend, the engine should keep *both* devices' edits — the
        // advantage over the LWW default, verified end-to-end through the sync pipeline.
        let cfg = SyncConfig {
            crdt_backend: crate::crdt::LORO_BACKEND.to_string(),
            ..SyncConfig::default()
        };
        let (_tmp, a, b) = two_devices_with(cfg);
        let note = a.book.new_note(ObjectType::Note, "shared").unwrap();
        let mut base = note.clone();
        base.body = "base.".into();
        a.book.save_note(&base).unwrap();
        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        let mut ea = a.book.store.read_note(&note.id).unwrap();
        ea.body = "base. APPEND-A".into();
        a.book.save_note(&ea).unwrap();
        let mut eb = b.book.store.read_note(&note.id).unwrap();
        eb.body = "PREPEND-B base.".into();
        b.book.save_note(&eb).unwrap();

        for _ in 0..4 {
            a.sync();
            b.sync();
        }
        a.book.store.refresh().unwrap();
        b.book.store.refresh().unwrap();
        let final_a = body_of(&a.book, &note.id);
        assert_eq!(final_a, body_of(&b.book, &note.id), "replicas converge");
        assert!(final_a.contains("APPEND-A"), "kept A's edit: {final_a:?}");
        assert!(final_a.contains("PREPEND-B"), "kept B's edit: {final_a:?}");
    }

    #[test]
    fn external_markdown_edit_is_ingested_into_the_sidecar() {
        let (_tmp, a, _b) = two_devices();
        let note = a.book.new_note(ObjectType::Note, "ext").unwrap();
        let mut n = note.clone();
        n.body = "original".into();
        a.book.save_note(&n).unwrap();
        a.sync();

        // Edit the markdown file directly (as an external tool would), then sync.
        let note_path = a
            .book
            .root
            .join(crate::storage::layout::note_filename(&note.id));
        let raw = std::fs::read_to_string(&note_path).unwrap();
        std::fs::write(&note_path, raw.replace("original", "edited externally")).unwrap();
        a.book.store.refresh().unwrap();
        a.sync();

        // The sidecar now reflects the external edit (reconcile folded markdown → CRDT).
        let sidecar = crate::storage::layout::crdt_sidecar_path(a.book.root.as_path(), &note.id);
        let backend = crate::crdt::select_crdt_backend(&a.cfg);
        let actor = actor_id_for(a.book.root.as_path()).unwrap();
        let doc = backend
            .load_document(&actor, &std::fs::read(&sidecar).unwrap())
            .unwrap();
        assert_eq!(doc.text(), "edited externally");
    }

    #[test]
    fn sync_survives_a_corrupted_local_note_without_deleting_it_remotely() {
        let (_tmp, a, b) = two_devices();
        let note = add_note(&a, "n", "hello");
        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();
        assert_eq!(body_of(&b.book, &note), "hello");

        // Hand-corrupt A's local copy directly on disk (bad YAML), bypassing the store.
        let note_path = a
            .book
            .root
            .join(crate::storage::layout::note_filename(&note));
        std::fs::write(&note_path, "---\nid: [ not valid\n---\nhello").unwrap();
        a.book.store.refresh().unwrap();

        // The pass must not error out, must record the file as skipped-unparseable (not silently
        // dropped), and must never treat it as deleted.
        let report = a.engine().sync().unwrap();
        assert!(report
            .skipped_unparseable
            .iter()
            .any(|p| p.ends_with(&crate::storage::layout::note_filename(&note))));
        assert!(report.deleted_remote.is_empty());

        // The remote still has a copy of the file (never deleted out from under B).
        let remote_path = a.remote.join(crate::storage::layout::note_filename(&note));
        assert!(remote_path.exists());

        // B can still sync without error too.
        assert!(b.engine().sync().is_ok());
    }

    #[test]
    fn merge_falls_back_to_conflict_when_the_local_note_is_unparseable() {
        let (_tmp, a, b) = two_devices();
        let note = add_note(&a, "n", "base");
        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        // B changes the note normally so the plan will want to Merge on the next pass...
        let mut eb = b.book.store.read_note(&note).unwrap();
        eb.body = "edit from B".into();
        b.book.save_note(&eb).unwrap();
        b.sync();

        // ...but A's local copy is corrupt, so there's no reliable body to merge against.
        let note_path = a
            .book
            .root
            .join(crate::storage::layout::note_filename(&note));
        std::fs::write(&note_path, "---\nid: [ not valid\n---\nhello").unwrap();
        a.book.store.refresh().unwrap();

        let report = a.engine().sync().unwrap();
        assert!(report.conflicted.iter().any(|p| p.ends_with(".md")));
        assert!(report.merged.is_empty());

        // A conflict copy preserving A's corrupt bytes exists alongside the winner.
        let has_conflict = std::fs::read_dir(a.book.root.as_path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("conflict"));
        assert!(has_conflict);
    }

    #[test]
    fn merge_falls_back_to_conflict_on_invalid_utf8_remote_note() {
        let (_tmp, a, b) = two_devices();
        let note = add_note(&a, "n", "hello");
        a.sync();
        b.sync();

        // A edits locally so the plan wants to Merge this note on the next pass.
        let mut ea = a.book.store.read_note(&note).unwrap();
        ea.body = "edit from A".into();
        a.book.save_note(&ea).unwrap();
        let local_bytes_before_conflict = std::fs::read(
            a.book
                .root
                .join(crate::storage::layout::note_filename(&note)),
        )
        .unwrap();

        // Directly corrupt the remote copy with invalid UTF-8 bytes and drop its sidecar, as if
        // an external tool wrote garbage with no CRDT record for it.
        let remote_note_path = a.remote.join(crate::storage::layout::note_filename(&note));
        let corrupt_remote_bytes = vec![b'h', b'i', 0xff, 0xfe];
        std::fs::write(&remote_note_path, &corrupt_remote_bytes).unwrap();
        std::fs::remove_file(crate::storage::layout::crdt_sidecar_path(&a.remote, &note)).unwrap();

        let report = a.engine().sync().unwrap();
        assert!(report.conflicted.iter().any(|p| p.ends_with(".md")));
        assert!(report.merged.is_empty());

        // Whichever variant "won" the byte-level conflict, it must be exactly one of the two
        // original byte sequences verbatim — never a synthesized `U+FFFD`-laden merge product.
        let local_now = std::fs::read(
            a.book
                .root
                .join(crate::storage::layout::note_filename(&note)),
        )
        .unwrap();
        assert!(
            local_now == local_bytes_before_conflict || local_now == corrupt_remote_bytes,
            "conflict resolution must preserve one variant byte-for-byte"
        );
    }

    #[test]
    fn corrupt_local_sidecar_is_rebuilt_from_markdown() {
        let (_tmp, a, _b) = two_devices();
        let note = add_note(&a, "n", "hello");
        a.sync(); // creates a good sidecar

        // Corrupt the sidecar directly on disk.
        let sidecar = crate::storage::layout::crdt_sidecar_path(a.book.root.as_path(), &note);
        std::fs::write(&sidecar, b"not a valid crdt snapshot").unwrap();

        let report = a.engine().sync().unwrap();
        assert!(report.rebuilt_sidecars.iter().any(|p| p.ends_with(".md")));

        // The note body itself is untouched by the rebuild.
        assert_eq!(body_of(&a.book, &note), "hello");
        // The sidecar is valid again and reflects the current body.
        let backend = crate::crdt::select_crdt_backend(&a.cfg);
        let actor = actor_id_for(a.book.root.as_path()).unwrap();
        let doc = backend
            .load_document(&actor, &std::fs::read(&sidecar).unwrap())
            .unwrap();
        assert_eq!(doc.text(), "hello");
    }

    #[cfg(feature = "loro")]
    #[test]
    fn loro_merge_survives_a_corrupted_local_sidecar() {
        // A corrupt local sidecar (rebuilt fresh from markdown, losing local CRDT history) merging
        // against a remote replica with real history must still converge without error or data
        // loss — the whole point of never propagating the load error out of the sync pass.
        let cfg = SyncConfig {
            crdt_backend: crate::crdt::LORO_BACKEND.to_string(),
            ..SyncConfig::default()
        };
        let (_tmp, a, b) = two_devices_with(cfg);
        let note = add_note(&a, "shared", "base.");
        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        let mut eb = b.book.store.read_note(&note).unwrap();
        eb.body = "PREPEND-B base.".into();
        b.book.save_note(&eb).unwrap();
        b.sync();

        // A also edits locally, then its sidecar is corrupted before the merge runs.
        let mut ea = a.book.store.read_note(&note).unwrap();
        ea.body = "base. APPEND-A".into();
        a.book.save_note(&ea).unwrap();
        let sidecar = crate::storage::layout::crdt_sidecar_path(a.book.root.as_path(), &note);
        std::fs::write(&sidecar, b"not a valid crdt snapshot").unwrap();

        for _ in 0..4 {
            assert!(a.engine().sync().is_ok());
            assert!(b.engine().sync().is_ok());
        }
        a.book.store.refresh().unwrap();
        b.book.store.refresh().unwrap();
        let final_a = body_of(&a.book, &note);
        assert_eq!(
            final_a,
            body_of(&b.book, &note),
            "replicas must still converge"
        );
        assert!(!final_a.is_empty(), "no data was lost outright");
    }

    // --- PIN-lock sync safety (privacy-security.md "PIN-Locked Notes", plan milestone M3) ---

    fn book_key() -> crate::pinlock::BookKey {
        crate::pinlock::BookKey::new([42u8; 32], "test0001".to_string())
    }

    fn lock_note(device: &Device, note: &mut crate::model::Note) {
        crate::pinlock::encrypt_note(note, &book_key()).unwrap();
        device.book.save_note(note).unwrap();
    }

    /// Every regular file under `root`, recursed (there's no `walkdir` dev-dependency; the remote
    /// test fixture is small enough that a hand-rolled walk is fine).
    fn all_files(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(root) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(all_files(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn locked_note_ciphertext_converges_across_devices() {
        let (_tmp, a, b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "diary").unwrap();
        note.summary = "private summary".into();
        note.body = "the secret plaintext".into();
        lock_note(&a, &mut note);

        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        let on_b = b.book.store.read_note(&note.id).unwrap();
        assert!(on_b.is_pin_locked());
        let decrypted = crate::pinlock::decrypt_note(&on_b, &book_key()).unwrap();
        assert_eq!(decrypted.body, "the secret plaintext");
        assert_eq!(decrypted.summary, "private summary");

        // Edit on B the way the tauri boundary would: decrypt, edit, re-encrypt via
        // `encrypt_for_save`, save — then propagate back to A.
        let mut edited = on_b;
        crate::pinlock::encrypt_for_save(&mut edited, "private summary", "edited on b", &book_key())
            .unwrap();
        b.book.save_note(&edited).unwrap();
        b.sync();
        a.sync();
        a.book.store.refresh().unwrap();

        let on_a = a.book.store.read_note(&note.id).unwrap();
        let decrypted_a = crate::pinlock::decrypt_note(&on_a, &book_key()).unwrap();
        assert_eq!(decrypted_a.body, "edited on b");
    }

    #[test]
    fn remote_never_carries_a_plaintext_sentinel_for_a_locked_note() {
        const SENTINEL: &[u8] = b"SENTINEL-PLAINTEXT-MUST-NOT-SYNC";
        let (_tmp, a, _b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "diary").unwrap();
        note.summary = "s SENTINEL-PLAINTEXT-MUST-NOT-SYNC s".into();
        note.body = "SENTINEL-PLAINTEXT-MUST-NOT-SYNC".into();
        lock_note(&a, &mut note);

        a.sync();

        for path in all_files(&a.remote) {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(SENTINEL.len())
                    .any(|window| window == SENTINEL),
                "plaintext sentinel leaked into remote file {path:?}"
            );
        }
    }

    #[test]
    fn resyncing_an_unchanged_locked_note_is_a_noop() {
        let (_tmp, a, b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "diary").unwrap();
        note.body = "locked content".into();
        lock_note(&a, &mut note);

        a.sync();
        b.sync();
        assert!(a.sync().is_noop(), "device A re-sync should be a no-op");
        assert!(b.sync().is_noop(), "device B re-sync should be a no-op");
    }

    #[test]
    fn concurrent_locked_note_edits_keep_ciphertext_and_nonce_metadata_paired() {
        let (_tmp, a, b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "diary").unwrap();
        note.body = "base secret".into();
        lock_note(&a, &mut note);

        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        let mut edit_a = a.book.store.read_note(&note.id).unwrap();
        crate::pinlock::encrypt_for_save(&mut edit_a, "", "secret from a", &book_key()).unwrap();
        a.book.save_note(&edit_a).unwrap();

        let mut edit_b = b.book.store.read_note(&note.id).unwrap();
        crate::pinlock::encrypt_for_save(&mut edit_b, "", "secret from b", &book_key()).unwrap();
        b.book.save_note(&edit_b).unwrap();

        a.sync();
        let report = b.sync();
        assert!(
            report.conflicted.iter().any(|path| path.ends_with(".md")),
            "locked note conflicts must use whole-file conflict resolution"
        );
        a.sync();
        a.book.store.refresh().unwrap();
        b.book.store.refresh().unwrap();

        let final_a = a.book.store.read_note(&note.id).unwrap();
        let final_b = b.book.store.read_note(&note.id).unwrap();
        let plain_a = crate::pinlock::decrypt_note(&final_a, &book_key()).unwrap();
        let plain_b = crate::pinlock::decrypt_note(&final_b, &book_key()).unwrap();
        assert_eq!(plain_a.body, plain_b.body);
        assert!(
            plain_a.body == "secret from a" || plain_a.body == "secret from b",
            "winner must be one complete encrypted variant, got {:?}",
            plain_a.body
        );
    }

    #[test]
    fn locked_note_and_plain_note_converge_together() {
        // A mixed sync pass (one locked note, one plain note) must not disturb either.
        let (_tmp, a, b) = two_devices();
        let mut locked = a.book.new_note(ObjectType::Note, "locked").unwrap();
        locked.body = "secret".into();
        lock_note(&a, &mut locked);
        let plain = add_note(&a, "plain", "not a secret");

        a.sync();
        b.sync();
        b.book.store.refresh().unwrap();

        assert_eq!(body_of(&b.book, &plain), "not a secret");
        let on_b = b.book.store.read_note(&locked.id).unwrap();
        assert!(on_b.is_pin_locked());
        assert_eq!(
            crate::pinlock::decrypt_note(&on_b, &book_key())
                .unwrap()
                .body,
            "secret"
        );
    }

    #[test]
    #[cfg(feature = "loro")]
    fn locked_note_sidecar_uses_lww_even_when_book_is_configured_for_loro() {
        let (_tmp, a, b) = two_devices_with(SyncConfig {
            crdt_backend: crate::crdt::LORO_BACKEND.to_string(),
            ..SyncConfig::default()
        });
        let mut locked = a.book.new_note(ObjectType::Note, "locked").unwrap();
        locked.body = "locked body".into();
        lock_note(&a, &mut locked);
        let mut plain = a.book.new_note(ObjectType::Note, "plain").unwrap();
        plain.body = "plain body under loro".into();
        a.book.save_note(&plain).unwrap();

        a.sync();
        b.sync();

        let actor = actor_id_for(&a.book.root).unwrap();
        let locked_sidecar_bytes =
            std::fs::read(crate::storage::layout::crdt_sidecar_path(&a.book.root, &locked.id))
                .unwrap();
        assert!(
            LwwBackend.load_document(&actor, &locked_sidecar_bytes).is_ok(),
            "a locked note's sidecar must be an LWW document regardless of the book's configured backend"
        );
        let plain_sidecar_bytes =
            std::fs::read(crate::storage::layout::crdt_sidecar_path(&a.book.root, &plain.id))
                .unwrap();
        assert!(
            LwwBackend.load_document(&actor, &plain_sidecar_bytes).is_err(),
            "an unlocked note under a Loro-configured book must actually use Loro, not LWW"
        );

        b.book.store.refresh().unwrap();
        let on_b = b.book.store.read_note(&locked.id).unwrap();
        assert_eq!(
            crate::pinlock::decrypt_note(&on_b, &book_key())
                .unwrap()
                .body,
            "locked body"
        );
    }

    #[test]
    fn pre_existing_loro_style_sidecar_is_rebuilt_from_ciphertext_body_once_locked() {
        // Simulate a note that was synced under Loro before it was ever locked (a garbage/foreign
        // sidecar from the LWW backend's point of view), then gets locked. The next reconcile pass
        // must detect the load failure and rebuild the sidecar from the (now-ciphertext) markdown
        // body — the same corrupt-sidecar migration path engine.rs already relies on.
        let (_tmp, a, _b) = two_devices();
        let mut note = a.book.new_note(ObjectType::Note, "migrating").unwrap();
        note.body = "will be locked".into();
        a.book.save_note(&note).unwrap();

        let sidecar_path = crate::storage::layout::crdt_sidecar_path(&a.book.root, &note.id);
        std::fs::write(&sidecar_path, b"not an lww snapshot, pretend loro binary").unwrap();

        lock_note(&a, &mut note);
        let report = a.sync();

        assert!(report.rebuilt_sidecars.iter().any(|p| p.ends_with(".md")));
        let actor = actor_id_for(&a.book.root).unwrap();
        let rebuilt = LwwBackend
            .load_document(&actor, &std::fs::read(&sidecar_path).unwrap())
            .unwrap();
        assert_eq!(rebuilt.text(), note.body, "rebuilt sidecar holds the ciphertext body");
    }
}
