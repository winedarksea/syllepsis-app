# Sync & Backup

## Design Goals

- High-quality backups with cross-device sharing.
- Users connect their own cloud storage — the app does not provide its own hosting.
- One user to one knowledge store is the primary design target; sharing with others is a nice-to-have extra.

## Cloud Storage Providers

Initial support: **Google Drive** and **GitHub**. Both can be active simultaneously. Users may in the future have other cloud storage and other git (say GitLab) options, but only ever one cloud drive, one git cloud. If both are present, git is treated as the lower priority target (not tracking every edit).

Example configuration:
- **Google Drive**: full backup; anyone with Drive permissions can access (aimed at 1–2 people, some read-only).
- **GitHub**: public-facing publish of non-private notes (private notes excluded via gitignore). The GitHub repo is the public version of the book.

Notes marked as **private** are excluded from the GitHub publish but included in the Google Drive backup.

## CRDT for Real-Time Sync

Near-real-time saves and cloud syncs use a CRDT library. Candidates:
- [yjs / y-rs](https://github.com/yjs/yjs)
- [automerge / autosurgeon](https://github.com/automerge/autosurgeon)
- [Loro](https://github.com/loro-dev/loro)

**Note:** CRDT does not track binary files (images). Images use UUID-based sidecars so the app handles file moves correctly regardless of path changes.

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
