//! Integrity-checking downloaded model files against the sha256 in their manifest.
//!
//! A model is a multi-gigabyte download a user trusts the app to run; a truncated or tampered
//! file should be caught before it is ever loaded into the runtime (llm-ai-features.md, "a
//! sha256-verified first-run download"). Hashing is deliberately separate from presence
//! ([`cache`](super::cache)): presence/size checks are the cheap per-open gate, while hashing runs
//! in the explicit download/repair path and right after a download completes. A file whose manifest entry has no
//! pinned hash yet is reported [`Unverified`](FileIntegrity::Unverified) rather than treated as
//! valid — honest about what was and wasn't checked, with no silent pass.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::CoreResult;
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::ModelManifest;

/// The outcome of checking one file against its expected hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileIntegrity {
    /// Hash present and matched.
    Verified,
    /// The manifest pinned no hash, so integrity could not be asserted (the file was still
    /// hashed-on-disk-existence by the caller, but its contents are unchecked).
    Unverified,
    /// Hash present and did **not** match — the file must be re-downloaded and never loaded.
    Mismatch { expected: String, actual: String },
}

impl FileIntegrity {
    /// Whether this outcome permits loading the file. `Unverified` is permitted (no hash to fail
    /// against); only a positive `Mismatch` blocks.
    pub fn is_loadable(&self) -> bool {
        !matches!(self, FileIntegrity::Mismatch { .. })
    }
}

/// Read length used while streaming a file through the hasher. 1 MiB balances syscall overhead
/// against memory for multi-GB weights without ever holding the whole file in memory.
const HASH_CHUNK_BYTES: usize = 1024 * 1024;

/// Stream `path` through SHA-256 and return the lowercase-hex digest.
pub fn hash_file(path: &Path) -> CoreResult<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

/// Hash `path` and compare against `expected` (lowercase hex). `None` short-circuits to
/// [`FileIntegrity::Unverified`] without hashing, since there is nothing to compare to.
pub fn verify_file(path: &Path, expected: Option<&str>) -> CoreResult<FileIntegrity> {
    let Some(expected) = expected else {
        return Ok(FileIntegrity::Unverified);
    };
    let actual = hash_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(FileIntegrity::Verified)
    } else {
        Ok(FileIntegrity::Mismatch {
            expected: expected.to_lowercase(),
            actual,
        })
    }
}

/// Verify every file of a cached model, returning `(file_name, outcome)` for each. Callers gate
/// loading on every outcome being [`FileIntegrity::is_loadable`].
pub fn verify_manifest(
    cache: &ModelCache,
    manifest: &ModelManifest,
) -> CoreResult<Vec<(String, FileIntegrity)>> {
    let mut out = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let path = cache.file_path(manifest, file);
        let outcome = verify_file(&path, file.sha256.as_deref())?;
        out.push((file.file_name().to_string(), outcome));
    }
    Ok(out)
}

/// Lowercase-hex encode bytes without pulling in a hex crate.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Writing to a String is infallible.
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // A well-formed but deliberately wrong digest, for the mismatch path.
    const WRONG_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn write_tmp(contents: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn hashes_a_known_vector() {
        // Verify against an independently computable digest of "abc".
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let (_d, p) = write_tmp(b"abc");
        assert_eq!(
            hash_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn matching_hash_verifies() {
        let (_d, p) = write_tmp(b"abc");
        let outcome = verify_file(
            &p,
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        )
        .unwrap();
        assert_eq!(outcome, FileIntegrity::Verified);
        assert!(outcome.is_loadable());
    }

    #[test]
    fn wrong_hash_is_a_blocking_mismatch() {
        let (_d, p) = write_tmp(b"abc");
        let outcome = verify_file(&p, Some(WRONG_SHA256)).unwrap();
        assert!(matches!(outcome, FileIntegrity::Mismatch { .. }));
        assert!(!outcome.is_loadable());
    }

    #[test]
    fn no_pinned_hash_is_unverified_but_loadable() {
        let (_d, p) = write_tmp(b"abc");
        let outcome = verify_file(&p, None).unwrap();
        assert_eq!(outcome, FileIntegrity::Unverified);
        assert!(outcome.is_loadable());
    }

    #[test]
    fn comparison_is_case_insensitive() {
        let (_d, p) = write_tmp(b"abc");
        let upper = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert_eq!(
            verify_file(&p, Some(upper)).unwrap(),
            FileIntegrity::Verified
        );
    }
}
