//! Inspecting model-cache readiness for management UI and diagnostics.
//!
//! Presence alone is not enough for model management: the UI needs to tell the user whether a
//! built-in model is missing, truncated, corrupt, or ready. This module keeps that inspection in
//! core so the Tauri shell only has to provide the OS app-data cache root.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::{FileRole, ModelKind, ModelManifest};
use crate::onnx::verify::{verify_file, FileIntegrity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFileCacheState {
    Missing,
    WrongSize,
    Present,
    Verified,
    Unverified,
    Mismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFileCacheStatus {
    pub file_name: String,
    pub repo_path: String,
    pub role: FileRole,
    pub expected_size_bytes: Option<u64>,
    pub actual_size_bytes: Option<u64>,
    pub sha256_configured: bool,
    pub state: ModelFileCacheState,
    pub mismatch_expected: Option<String>,
    pub mismatch_actual: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCacheStatus {
    pub model_id: String,
    pub display_name: String,
    pub kind: ModelKind,
    /// True when all files are present and match pinned byte sizes. This is the cheap provider
    /// selection gate.
    pub cached: bool,
    /// True when all files are present, size-valid, and have no hash mismatch. This only proves
    /// hashes when `verify_hashes` was true.
    pub loadable: bool,
    pub files: Vec<ModelFileCacheStatus>,
}

pub fn inspect_model_cache(
    cache: &ModelCache,
    manifest: &ModelManifest,
    verify_hashes: bool,
) -> CoreResult<ModelCacheStatus> {
    let mut files = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let path = cache.file_path(manifest, file);
        let metadata = path.metadata().ok();
        let actual_size_bytes = metadata
            .as_ref()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len());

        let mut status = ModelFileCacheStatus {
            file_name: file.file_name().to_string(),
            repo_path: file.repo_path.clone(),
            role: file.role,
            expected_size_bytes: file.size_bytes,
            actual_size_bytes,
            sha256_configured: file.sha256.is_some(),
            state: ModelFileCacheState::Missing,
            mismatch_expected: None,
            mismatch_actual: None,
        };

        status.state = match actual_size_bytes {
            None => ModelFileCacheState::Missing,
            Some(actual_size)
                if file
                    .size_bytes
                    .is_some_and(|expected_size| expected_size != actual_size) =>
            {
                ModelFileCacheState::WrongSize
            }
            Some(_) if verify_hashes => match verify_file(&path, file.sha256.as_deref())? {
                FileIntegrity::Verified => ModelFileCacheState::Verified,
                FileIntegrity::Unverified => ModelFileCacheState::Unverified,
                FileIntegrity::Mismatch { expected, actual } => {
                    status.mismatch_expected = Some(expected);
                    status.mismatch_actual = Some(actual);
                    ModelFileCacheState::Mismatch
                }
            },
            Some(_) => ModelFileCacheState::Present,
        };

        files.push(status);
    }

    let cached = files.iter().all(|file| {
        !matches!(
            file.state,
            ModelFileCacheState::Missing | ModelFileCacheState::WrongSize
        )
    });
    let loadable = cached
        && files
            .iter()
            .all(|file| !matches!(file.state, ModelFileCacheState::Mismatch));

    Ok(ModelCacheStatus {
        model_id: manifest.id.clone(),
        display_name: manifest.display_name.clone(),
        kind: manifest.kind,
        cached,
        loadable,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx::execution_provider::ExecutionProvider;
    use crate::onnx::manifest::{FileRole, ModelFile, ModelManifest, Quantization};
    use std::fs;

    fn manifest() -> ModelManifest {
        ModelManifest {
            id: "test-model".to_string(),
            display_name: "Test Model".to_string(),
            repo: "owner/repo".to_string(),
            revision: "main".to_string(),
            kind: ModelKind::Embedding,
            quantization: Quantization::Int8,
            files: vec![
                ModelFile {
                    repo_path: "model.onnx".to_string(),
                    role: FileRole::Weights,
                    sha256: Some(
                        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                            .to_string(),
                    ),
                    size_bytes: Some(3),
                },
                ModelFile {
                    repo_path: "config.json".to_string(),
                    role: FileRole::Config,
                    sha256: None,
                    size_bytes: Some(2),
                },
            ],
            hidden_size: 3,
            max_context_tokens: 8,
            min_ram_mb: 1,
            preferred_execution_providers: vec![ExecutionProvider::Cpu],
            pooling: None,
            query_instruction: None,
        }
    }

    #[test]
    fn reports_missing_and_wrong_size_without_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let manifest = manifest();
        let path = cache.file_path(&manifest, &manifest.files[0]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"too-long").unwrap();

        let status = inspect_model_cache(&cache, &manifest, false).unwrap();

        assert!(!status.cached);
        assert!(!status.loadable);
        assert_eq!(status.files[0].state, ModelFileCacheState::WrongSize);
        assert_eq!(status.files[1].state, ModelFileCacheState::Missing);
    }

    #[test]
    fn verifies_hashes_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let manifest = manifest();
        fs::create_dir_all(cache.model_dir(&manifest)).unwrap();
        fs::write(cache.file_path(&manifest, &manifest.files[0]), b"abc").unwrap();
        fs::write(cache.file_path(&manifest, &manifest.files[1]), b"{}").unwrap();

        let status = inspect_model_cache(&cache, &manifest, true).unwrap();

        assert!(status.cached);
        assert!(status.loadable);
        assert_eq!(status.files[0].state, ModelFileCacheState::Verified);
        assert_eq!(status.files[1].state, ModelFileCacheState::Unverified);
    }

    #[test]
    fn reports_hash_mismatch_as_not_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModelCache::new(dir.path());
        let manifest = manifest();
        fs::create_dir_all(cache.model_dir(&manifest)).unwrap();
        fs::write(cache.file_path(&manifest, &manifest.files[0]), b"bad").unwrap();
        fs::write(cache.file_path(&manifest, &manifest.files[1]), b"{}").unwrap();

        let status = inspect_model_cache(&cache, &manifest, true).unwrap();

        assert!(status.cached);
        assert!(!status.loadable);
        assert_eq!(status.files[0].state, ModelFileCacheState::Mismatch);
        assert!(status.files[0].mismatch_actual.is_some());
    }
}
