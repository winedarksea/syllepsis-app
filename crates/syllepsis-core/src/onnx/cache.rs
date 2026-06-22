//! Where downloaded model files live on disk, and whether a given model is already present.
//!
//! Models are large and book-independent, so they are cached once in an OS app-data directory
//! shared across every book — not inside any book folder (llm-ai-features.md, "an OS app-data
//! models directory shared across books"). The layout is flat and predictable:
//!
//! ```text
//! <root>/<model-id>/<file-name>
//! ```
//!
//! so a model is `is_cached` exactly when every file its manifest names exists. The root is
//! injected rather than discovered here, keeping this layer pure (no env, no platform dirs) and
//! letting tests point it at a tempdir; the Tauri shell passes the real app-data path.

use std::path::{Path, PathBuf};

use crate::onnx::manifest::{ModelFile, ModelManifest};

/// A handle to the on-disk model cache rooted at a single directory.
#[derive(Debug, Clone)]
pub struct ModelCache {
    root: PathBuf,
}

impl ModelCache {
    /// Open a cache rooted at `root` (e.g. `~/Library/Application Support/Syllepsis/models`).
    /// The directory is not created until a file is actually written.
    pub fn new(root: impl Into<PathBuf>) -> ModelCache {
        ModelCache { root: root.into() }
    }

    /// The cache root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory holding one model's files.
    pub fn model_dir(&self, manifest: &ModelManifest) -> PathBuf {
        self.root.join(&manifest.id)
    }

    /// The on-disk path for one file of a model (named by its bare file name, not its repo path).
    pub fn file_path(&self, manifest: &ModelManifest, file: &ModelFile) -> PathBuf {
        self.model_dir(manifest).join(file.file_name())
    }

    /// Whether every file the manifest names is present on disk. Presence only — integrity is
    /// [`verify`](super::verify)'s job, kept separate because hashing is expensive and presence
    /// is the cheap gate the provider-selection path checks on every open.
    pub fn is_cached(&self, manifest: &ModelManifest) -> bool {
        manifest
            .files
            .iter()
            .all(|f| self.file_path(manifest, f).is_file())
    }

    /// The files a manifest names that are *not* yet on disk — the download work list.
    pub fn missing_files<'a>(&self, manifest: &'a ModelManifest) -> Vec<&'a ModelFile> {
        manifest
            .files
            .iter()
            .filter(|f| !self.file_path(manifest, f).is_file())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx::manifest::builtin;
    use crate::onnx::manifest::BUNDLED_LLM_ID;
    use std::fs;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn paths_are_namespaced_by_model_id() {
        let cache = ModelCache::new("/tmp/models");
        let m = builtin(BUNDLED_LLM_ID).unwrap();
        let weights = m.llm_decoder_graph_file().unwrap();
        let p = cache.file_path(&m, weights);
        assert!(p.ends_with("gemma-4-e2b/decoder_model_merged_q4.onnx"));
    }

    #[test]
    fn reports_missing_then_cached_as_files_appear() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let m = builtin(BUNDLED_LLM_ID).unwrap();

        assert!(!cache.is_cached(&m));
        assert_eq!(cache.missing_files(&m).len(), m.files.len());

        // Materialize all but one file: still not cached.
        for f in m.files.iter().skip(1) {
            touch(&cache.file_path(&m, f));
        }
        assert!(!cache.is_cached(&m));
        assert_eq!(cache.missing_files(&m).len(), 1);

        // Materialize the last: now cached, nothing missing.
        touch(&cache.file_path(&m, &m.files[0]));
        assert!(cache.is_cached(&m));
        assert!(cache.missing_files(&m).is_empty());
    }
}
