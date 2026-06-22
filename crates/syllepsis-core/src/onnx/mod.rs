//! The shared ONNX model-runtime pattern: one set of machinery for fetching, verifying,
//! caching, placing, and (behind the `onnx` feature) running local models — used by **both** the
//! embedder and the LLM.
//!
//! The design decision driving this module is that the local LLM and the local embedder run on
//! the *same* ONNX Runtime stack, so they should not each reinvent model management. Everything
//! a model needs is described once by a [`ModelManifest`]; from there a [`ModelCache`] places its
//! files, [`download`] fetches the missing ones through a swappable [`ModelFetcher`],
//! [`verify`] checks them against pinned sha256s, and [`execution_provider`] picks the backend.
//! Gemma 4 E2B and Qwen3-Embedding are different model families, but the local runtime mechanics
//! are intentionally the same: manifest-described ONNX exports, tokenizer files, external data,
//! cache verification, and execution-provider selection.
//!
//! Split by compile cost: the manifest/cache/verify/download-plan/EP-selection layer is pure and
//! always compiled (and unit-tested with no network and no model bytes). The actual `ort`
//! session, the Hugging Face tokenizer, and the HTTP fetcher pull heavy native dependencies and
//! live behind the `onnx` cargo feature, so the default build and test suite stay fast and
//! offline. Embeddings still have an offline hashing implementation; LLM generation requires a
//! real local or cloud/server model.

pub mod cache;
pub mod download;
pub mod execution_provider;
pub mod manifest;
pub mod status;
pub mod verify;

#[cfg(feature = "onnx")]
pub mod http;
#[cfg(feature = "onnx")]
pub mod session;
#[cfg(feature = "onnx")]
pub mod tokenizer;

use serde::{Deserialize, Serialize};

pub use cache::ModelCache;
pub use download::{download_missing, file_url, plan_download, DownloadItem, ModelFetcher};
pub use execution_provider::{
    select_execution_provider, ExecutionProvider, ExecutionProviderChoice, Platform,
};
pub use manifest::{
    builtin, builtin_manifests, ModelFile, ModelKind, ModelManifest, PoolingStrategy, Quantization,
    BUNDLED_LLM_ID, QWEN3_EMBEDDING_ID,
};
pub use status::{
    inspect_model_cache, ModelCacheStatus, ModelFileCacheState, ModelFileCacheStatus,
};
pub use verify::{verify_manifest, FileIntegrity};

#[cfg(feature = "onnx")]
pub use http::HttpModelFetcher;

/// A snapshot of how a local model is set up to run, surfaced in the Diagnostics view so the user
/// can see which model, precision, and hardware backend are in play — and whether inference fell
/// back to CPU (i.e. will be slow).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDiagnostics {
    pub model_id: String,
    pub display_name: String,
    pub kind: ModelKind,
    pub quantization: Quantization,
    pub execution_provider: ExecutionProvider,
    pub used_cpu_fallback: bool,
}

impl RuntimeDiagnostics {
    /// Combine a manifest with the chosen execution provider into a diagnostics record.
    pub fn new(manifest: &ModelManifest, choice: &ExecutionProviderChoice) -> RuntimeDiagnostics {
        RuntimeDiagnostics {
            model_id: manifest.id.clone(),
            display_name: manifest.display_name.clone(),
            kind: manifest.kind,
            quantization: manifest.quantization,
            execution_provider: choice.provider,
            used_cpu_fallback: choice.used_cpu_fallback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_carry_model_and_backend() {
        let m = builtin(BUNDLED_LLM_ID).unwrap();
        let choice = select_execution_provider(
            &m.preferred_execution_providers,
            &[ExecutionProvider::CoreMl],
            Platform::MacOs,
        );
        let diag = RuntimeDiagnostics::new(&m, &choice);
        assert_eq!(diag.model_id, BUNDLED_LLM_ID);
        assert_eq!(diag.execution_provider, ExecutionProvider::CoreMl);
        assert!(!diag.used_cpu_fallback);
    }
}
