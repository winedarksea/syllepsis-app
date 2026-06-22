//! Planning and orchestrating the first-run download of a model's files.
//!
//! The two halves are split for testability. *Planning* — turning a manifest plus what is
//! already cached into a list of (url, destination, expected-hash) work items — is pure and
//! always compiled. *Fetching* the bytes is the side-effecting part, hidden behind the
//! [`ModelFetcher`] seam so the orchestration ([`download_missing`]) can be driven by a fake in
//! tests and by a real HTTP client (feature `onnx`, see [`http`](super::http)) in the app. Cached
//! files with pinned hashes are verified before planning, and freshly fetched files are verified
//! before they are accepted; a hash mismatch aborts loudly rather than leaving a corrupt model in
//! the cache.

use std::path::PathBuf;

use crate::error::{CoreError, CoreResult};
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::{ModelFile, ModelManifest};
use crate::onnx::verify::{verify_file, FileIntegrity};

/// Hugging Face's file-resolve host. Files are fetched from
/// `{HOST}/{repo}/resolve/{revision}/{repo_path}`.
const HF_HOST: &str = "https://huggingface.co";

/// The download URL for one file of a model, pinned to the manifest's revision.
pub fn file_url(manifest: &ModelManifest, file: &ModelFile) -> String {
    format!(
        "{HF_HOST}/{}/resolve/{}/{}",
        manifest.repo, manifest.revision, file.repo_path
    )
}

/// One unit of download work: where to get the bytes, where they go, and what they must hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadItem {
    pub url: String,
    pub dest: PathBuf,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
}

/// The work list to make `manifest` fully present in `cache`: one item per missing file, empty
/// when the model is already cached. Pure — it only reads the filesystem to check presence.
pub fn plan_download(cache: &ModelCache, manifest: &ModelManifest) -> Vec<DownloadItem> {
    cache
        .missing_files(manifest)
        .into_iter()
        .map(|file| DownloadItem {
            url: file_url(manifest, file),
            dest: cache.file_path(manifest, file),
            sha256: file.sha256.clone(),
            size_bytes: file.size_bytes,
        })
        .collect()
}

/// Fetches a single file's bytes to disk. The one boundary the orchestration crosses, so the
/// HTTP client (and its heavy dependency) stays optional and out of the always-compiled path.
pub trait ModelFetcher {
    /// Download `item.url` to `item.dest`. The destination's parent directory already exists.
    /// Implementations must write atomically enough that a failure does not leave a partial file
    /// that would later read as "present" — write to a temp path and rename, or remove on error.
    fn fetch(&self, item: &DownloadItem) -> CoreResult<()>;
}

/// Download every missing file of `manifest` via `fetcher`, then verify each freshly fetched
/// file against its pinned hash. Returns the integrity outcome per downloaded file. A
/// [`FileIntegrity::Mismatch`] becomes a [`CoreError::Model`] so a corrupt download can never be
/// silently loaded; the bad file is removed first so the next run re-fetches it.
pub fn download_missing(
    cache: &ModelCache,
    manifest: &ModelManifest,
    fetcher: &dyn ModelFetcher,
) -> CoreResult<Vec<(String, FileIntegrity)>> {
    std::fs::create_dir_all(cache.model_dir(manifest))?;
    remove_mismatched_cached_files(cache, manifest)?;
    let plan = plan_download(cache, manifest);
    if plan.is_empty() {
        tracing::info!(
            model = %manifest.id,
            cache_root = %cache.root().display(),
            "model cache already complete; skipping download"
        );
        return Ok(Vec::new());
    }
    tracing::info!(
        model = %manifest.id,
        cache_root = %cache.root().display(),
        files = plan.len(),
        "downloading missing model files"
    );

    let mut report = Vec::with_capacity(plan.len());
    for item in &plan {
        let name = item.dest.file_name().and_then(|s| s.to_str()).unwrap_or("");
        tracing::info!(
            model = %manifest.id,
            file = name,
            "downloading model file"
        );
        fetcher.fetch(item)?;
        verify_downloaded_size(item)?;
        let integrity = verify_file(&item.dest, item.sha256.as_deref())?;
        if let FileIntegrity::Mismatch { expected, actual } = &integrity {
            // Don't leave a corrupt file masquerading as present.
            let _ = std::fs::remove_file(&item.dest);
            return Err(CoreError::Model(format!(
                "sha256 mismatch for {}: expected {expected}, got {actual}",
                item.dest.display()
            )));
        }
        report.push((name.to_string(), integrity));
    }
    Ok(report)
}

fn verify_downloaded_size(item: &DownloadItem) -> CoreResult<()> {
    let Some(expected_size) = item.size_bytes else {
        return Ok(());
    };
    let actual_size = item.dest.metadata()?.len();
    if actual_size == expected_size {
        return Ok(());
    }
    let _ = std::fs::remove_file(&item.dest);
    Err(CoreError::Model(format!(
        "size mismatch for {}: expected {expected_size} bytes, got {actual_size}",
        item.dest.display()
    )))
}

fn remove_mismatched_cached_files(cache: &ModelCache, manifest: &ModelManifest) -> CoreResult<()> {
    for file in &manifest.files {
        let path = cache.file_path(manifest, file);
        if !path.is_file() {
            continue;
        }
        let integrity = verify_file(&path, file.sha256.as_deref())?;
        if let FileIntegrity::Mismatch { .. } = integrity {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx::manifest::{builtin, BUNDLED_LLM_ID};
    use std::cell::RefCell;

    #[test]
    fn url_is_pinned_to_repo_and_revision() {
        let m = builtin(BUNDLED_LLM_ID).unwrap();
        let weights = m.llm_decoder_graph_file().unwrap();
        assert_eq!(
            file_url(&m, weights),
            "https://huggingface.co/onnx-community/gemma-4-E2B-it-ONNX/resolve/main/onnx/decoder_model_merged_q4.onnx"
        );
    }

    #[test]
    fn plan_covers_all_files_when_nothing_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let m = builtin(BUNDLED_LLM_ID).unwrap();
        assert_eq!(plan_download(&cache, &m).len(), m.files.len());
    }

    /// A fetcher that writes fixed bytes and records the order it was asked for files.
    struct FakeFetcher {
        contents: &'static [u8],
        fetched: RefCell<Vec<String>>,
    }

    impl ModelFetcher for FakeFetcher {
        fn fetch(&self, item: &DownloadItem) -> CoreResult<()> {
            self.fetched.borrow_mut().push(
                item.dest
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
            std::fs::write(&item.dest, self.contents)?;
            Ok(())
        }
    }

    #[test]
    fn downloads_every_missing_file_then_skips_cached_ones() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let mut m = builtin(BUNDLED_LLM_ID).unwrap();
        for file in &mut m.files {
            file.sha256 =
                Some("9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c".into());
            file.size_bytes = Some(7);
        }
        let fetcher = FakeFetcher {
            contents: b"weights",
            fetched: RefCell::new(Vec::new()),
        };

        let report = download_missing(&cache, &m, &fetcher).unwrap();
        assert_eq!(report.len(), m.files.len());
        assert!(cache.is_cached(&m), "all files should now be present");
        assert!(report.iter().all(|(_, i)| *i == FileIntegrity::Verified));

        // A second pass plans nothing and fetches nothing.
        let again = download_missing(&cache, &m, &fetcher).unwrap();
        assert!(again.is_empty());
        assert_eq!(fetcher.fetched.borrow().len(), m.files.len());
    }

    /// A fetcher whose bytes won't match a (hypothetical) pinned hash.
    struct CorruptFetcher;
    impl ModelFetcher for CorruptFetcher {
        fn fetch(&self, item: &DownloadItem) -> CoreResult<()> {
            std::fs::write(&item.dest, b"corrupt")?;
            Ok(())
        }
    }

    #[test]
    fn hash_mismatch_aborts_and_removes_the_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let mut m = builtin(BUNDLED_LLM_ID).unwrap();
        // Pin a hash the corrupt bytes can't satisfy.
        m.files[0].sha256 =
            Some("1111111111111111111111111111111111111111111111111111111111111111".to_string());

        let err = download_missing(&cache, &m, &CorruptFetcher).unwrap_err();
        assert!(matches!(err, CoreError::Model(_)));
        // The corrupt file was removed, so the model is not (wrongly) considered cached.
        assert!(!cache.file_path(&m, &m.files[0]).exists());
    }

    #[test]
    fn size_mismatch_aborts_and_removes_the_bad_file_even_without_hash() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let mut m = builtin(BUNDLED_LLM_ID).unwrap();
        m.files.truncate(1);
        m.files[0].sha256 = None;
        m.files[0].size_bytes = Some(99);
        let fetcher = FakeFetcher {
            contents: b"weights",
            fetched: RefCell::new(Vec::new()),
        };

        let err = download_missing(&cache, &m, &fetcher).unwrap_err();

        assert!(err.to_string().contains("size mismatch"));
        assert!(!cache.file_path(&m, &m.files[0]).exists());
    }

    #[test]
    fn present_hash_mismatch_is_repaired_by_refetching_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let mut m = builtin(BUNDLED_LLM_ID).unwrap();
        for file in &mut m.files {
            file.sha256 =
                Some("9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c".into());
            file.size_bytes = Some(7);
        }
        for file in &m.files {
            let path = cache.file_path(&m, file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"weights").unwrap();
        }
        std::fs::write(cache.file_path(&m, &m.files[0]), b"corrupt").unwrap();
        let fetcher = FakeFetcher {
            contents: b"weights",
            fetched: RefCell::new(Vec::new()),
        };

        let report = download_missing(&cache, &m, &fetcher).unwrap();

        assert_eq!(report.len(), 1);
        assert_eq!(
            fetcher.fetched.borrow().as_slice(),
            ["embed_tokens_q4.onnx"]
        );
        assert!(cache.is_cached(&m));
    }
}
