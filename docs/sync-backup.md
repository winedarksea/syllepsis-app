# Sync & Backup

## Design Goals

- High-quality backups with cross-device sharing.
- Users connect their own cloud storage — the app does not provide its own hosting.
- One user to one knowledge store is the primary design target; sharing with others is a nice-to-have extra.

## Cloud Storage Providers

Initial support: **Google Drive** and **GitHub**. Both can be active simultaneously.

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
