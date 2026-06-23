# Sync & Backup

## Design Goals

- High-quality backups with cross-device sharing.
- Users connect their own cloud storage — the app does not provide its own hosting.
- One user to one knowledge store is the primary design target; sharing with others is a nice-to-have extra.

## Cloud Storage Providers

There are three main ways to do cloud sync here: git, file watching (no active management), and managed cloud sync.
Users could use both git and either file watching or a cloud drive for sync at the same time (or just one of the two, or neither). Generally the idea is git is used for public publishing (with gitignore to hide private notes) or for more formal "version" release cadences, while a cloud drive sync is for more real time sync, shared between devices.

Example configuration:
- **Google Drive**: full backup; anyone with Drive permissions can access (aimed at 1–2 people, some read-only).
- **GitHub**: public-facing publish of non-private notes (private notes excluded via gitignore). The GitHub repo is the public version of the book.
- **File Watching with File Sync and Share**: Users can chose to create a book in a folder that is already cloud managed (ie by Google Drive Desktop App, Apple Cloud, etc). In this case, we don't push or pull anything from the app, however we track with notify updates to the local folder, and use Loro to manage the conflicts, automatically cleaning them up. We will still have a UI view here that should track and show external updates and conflicts, so users can be aware of what is happening here.

Notes marked as **private** are excluded from the GitHub publish but included in the Google Drive backup.

## CRDT for Real-Time Sync

Near-real-time saves and cloud syncs use a CRDT library. Candidates:
- [yjs / y-rs](https://github.com/yjs/yjs)
- [automerge / autosurgeon](https://github.com/automerge/autosurgeon)
- [Loro](https://github.com/loro-dev/loro)

**Note:** CRDT does not track binary files (images). Images use UUID-based sidecars so the app handles file moves correctly regardless of path changes.

### Phase 4 implementation (in place)

Built behind two seams mirroring the embeddings/LLM pattern (`crates/syllepsis-core/src/crdt`, `…/src/sync`):

- **`NoteCrdt` / `CrdtBackend`** — per-note convergent documents. Each note carries a CRDT **sidecar** at `_crdt/{ulid}.crdt` (keyed on the ulid so a retitle/move never orphans it). The sidecar — not the markdown — is the cross-device merge authority; markdown is the source of truth for *local* edits and is rendered from the merged sidecar.
  - **Default backend `lww`** (always compiled): a last-writer-wins register over the whole body, keyed by a hybrid logical clock `(wall_ms, counter, actor)`. A genuine CRDT (total-order `max` — commutative/associative/idempotent), correct for the primary one-user/many-devices target. Whole-body winner, not paragraph-level merge.
  - **Backend `loro`** (optional `loro` Cargo feature): the fine-grained text CRDT — concurrent edits to *different* regions of a note both survive.
- **`SyncProvider`** — the user-owned remote reduced to list/get/put/delete over book-relative paths. Default impl **`LocalFolderSync`** (a synced folder is exactly how the Drive/Dropbox desktop clients expose the cloud; revision = content sha256). Google Drive / GitHub are advertised in `provider_descriptors()` for the UI but not yet wired (honestly flagged `implemented: false`).
- **`SyncEngine`** — one pass: reconcile markdown→sidecars, fingerprint local + list remote, run a **pure planner** (`sync::plan`) over per-file state, then apply push/pull/**merge**/**conflict**/delete. Concurrent note bodies merge through their sidecars; non-mergeable files (categories, `_book.md`, …) get a deterministic `.conflict-{hash}` copy (winner = greater content hash, so both devices converge on the same pair). Per-device **`SyncState`** under `_sync/` (never synced) records last-synced fingerprints so an unchanged file is skipped — **infinite-write-loop prevention** is "skip when neither side changed".

Storage: `_crdt/` (sidecars) is synced but git-ignored; `_sync/` (per-device state + actor id) is local-only. Both are excluded from the note scan. Binary assets are tracked by UUID sidecars (`sync::assets`), not CRDT'd.

Known POC limitations: concurrent *frontmatter* (metadata) edits last-writer-win (only the body merges); the cloud HTTP providers and git integration are not implemented yet.

### Conflict Management
- Actively manage cloud conflict files: merge and delete (identified by UUID).
- Implement mitigations to prevent infinite write loops.

### Drawings and SVG
[Drawings/SVG](object-types.md#drawings) are text (XML), so unlike raster images they *can* be diffed and merged — but CRDT-tracking arbitrary SVG geometry is messy (verbose path data, structural reordering produces conflict noise) and the CRDT layer is tuned for the note-text model. The decision:

- **Geometry** (the SVG/drawing itself) is synced as a **file with a UUID sidecar**, like images — not in the live CRDT document. Because it is text, it still diffs cleanly at Git commit boundaries.
- **Overlay anchors** (which note/category links to which point or region, and the coordinates) live in note metadata and the worlds registry, which **are** CRDT-tracked. This keeps the merge-sensitive, small, structured data in the CRDT layer and the heavy geometry in file sync.

Open door: because app-authored drawings have a structure the app controls, simple in-app drawings *could* later be CRDT-tracked; imported third-party SVGs stay file-synced. See [open-questions.md](open-questions.md).

## Git Integration

Git is a dependency but used differently from standard version control:
- **Commits** are more like official versions or releases — deliberate, meaningful snapshots.
- **Saves and cloud syncs** (e.g. to Drive) happen in near-real time, below the commit level.

Potential Rust crate: `git2`.

## Data Integrity & Testing

Users must be able to trust they will not lose insights. This requires:
- Good test coverage of sync and merge paths.
- Clear recovery workflows for corrupted or conflicting states.

See [platform-infra.md](platform-infra.md) for testing philosophy.
