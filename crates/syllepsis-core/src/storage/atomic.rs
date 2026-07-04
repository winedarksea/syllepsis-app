//! Atomic file writes: temp file in the same directory, fsync, rename over the target. This is
//! what stops a crash mid-write from ever leaving a truncated note (or sidecar, or manifest) on
//! disk — the reader either sees the old bytes or the new ones, never a partial write.
//!
//! Temp files are named `{filename}.tmp-{pid}-{seq}`; every reserved-dir/extension scan in the
//! codebase (`collect_note_files`, category/world scans, etc.) already filters on exact
//! extensions, so a `.tmp-...` leftover from a crash is invisible to them without extra code.
//! `sync::is_local_only` also excludes `.tmp-` paths so a crash leftover can never be pushed to a
//! remote.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::CoreResult;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Atomically replace `path` with `bytes`: write to a temp file in the same directory, fsync it,
/// then rename over the target. Creates the parent directory if it doesn't exist.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let temp = temp_path(path);
    let result = (|| -> CoreResult<()> {
        {
            let mut file = File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        rename_over(&temp, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }

    #[cfg(unix)]
    if result.is_ok() {
        // Best-effort: fsync the parent directory so the rename itself is durable. Not fatal if
        // it fails (e.g. platforms/filesystems that don't support opening a directory).
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    result
}

fn temp_path(path: &Path) -> PathBuf {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!("{file_name}.tmp-{}-{seq}", std::process::id()))
}

/// Rename `temp` over `target`, retrying once after removing an existing target. `fs::rename` is
/// atomic-replace on Unix, but on Windows it fails if the target exists.
fn rename_over(temp: &Path, target: &Path) -> CoreResult<()> {
    if let Err(error) = fs::rename(temp, target) {
        if target.exists() {
            fs::remove_file(target)?;
            fs::rename(temp, target)?;
        } else {
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn overwrites_existing_with_no_tmp_siblings_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
    }

    #[test]
    fn creates_missing_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/note.md");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn leftover_temp_is_ignored_by_note_scan() {
        use crate::model::{Note, ObjectType};
        use crate::storage::{FsNoteStore, NoteStore};

        let dir = tempfile::tempdir().unwrap();
        let store = FsNoteStore::open(dir.path()).unwrap();
        let mut note = Note::new(ObjectType::Note, "n", "syllepsis_001");
        note.body = "x".into();
        store.write_note(&note).unwrap();

        // Simulate a crash leftover next to the note.
        let leftover = dir.path().join("leftover.md.tmp-1-1");
        fs::write(&leftover, b"partial").unwrap();
        store.refresh().unwrap();

        assert_eq!(store.read_all_notes().unwrap().len(), 1);
    }

    #[test]
    fn leftover_temp_is_local_only() {
        assert!(crate::sync::is_local_only("notes/note.md.tmp-123-4"));
    }
}
