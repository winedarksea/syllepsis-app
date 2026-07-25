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
use std::thread;
use std::time::Duration;

use crate::error::CoreResult;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Total `fs::rename` attempts before considering the destructive fallback. Both platforms
/// replace an existing target in one atomic call (Unix natively, Windows via
/// `MOVEFILE_REPLACE_EXISTING`), so a failure here means something transient is holding the file
/// open — typically an antivirus scanner or search indexer on Windows.
const RENAME_ATTEMPT_LIMIT: u32 = 4;

/// Pause between rename attempts. Sharing violations clear in milliseconds, so the whole retry
/// budget stays well under a frame while still riding out a scanner's grab on the file.
const RENAME_RETRY_BACKOFF: Duration = Duration::from_millis(20);

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

/// Rename `temp` over `target`, preferring the atomic single-call replace and only degrading to
/// remove-then-rename as a last resort.
///
/// A plain `fs::rename` already replaces an existing target on both platforms — Unix natively,
/// Windows because std passes `MOVEFILE_REPLACE_EXISTING` — so the destructive fallback is
/// exceptional, not the normal path for an existing file. The realistic cause of failure is a
/// transient Windows sharing violation (antivirus or the search indexer holding the target open
/// for a moment), which a short retry rides out while keeping the replace atomic. Only after the
/// retries are exhausted do we unlink the target, because that opens a window where the note
/// exists nowhere on disk.
fn rename_over(temp: &Path, target: &Path) -> CoreResult<()> {
    let mut attempts_remaining = RENAME_ATTEMPT_LIMIT;
    let last_error = loop {
        match fs::rename(temp, target) {
            Ok(()) => return Ok(()),
            Err(error) => {
                attempts_remaining -= 1;
                // Retrying only helps while the replacement still exists; if the temp file is gone
                // the write has already failed and nothing can be salvaged.
                if attempts_remaining == 0 || !temp.exists() {
                    break error;
                }
                thread::sleep(RENAME_RETRY_BACKOFF);
            }
        }
    };

    // Never unlink the target unless the replacement is still on disk to take its place —
    // otherwise a failing write would destroy the user's existing file outright.
    if temp.exists() && target.exists() {
        fs::remove_file(target)?;
        fs::rename(temp, target)?;
        return Ok(());
    }
    Err(last_error.into())
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
    fn rename_over_leaves_target_intact_when_the_replacement_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"precious").unwrap();

        let vanished_temp = dir.path().join("note.md.tmp-0-0");
        assert!(rename_over(&vanished_temp, &target).is_err());
        // The remove-then-rename fallback must never leave the note deleted with no replacement.
        assert_eq!(fs::read(&target).unwrap(), b"precious");
    }

    #[test]
    fn leftover_temp_is_local_only() {
        assert!(crate::sync::is_local_only("notes/note.md.tmp-123-4"));
    }
}
