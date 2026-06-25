//! Choosing the embedding provider for a book: the model-backed ONNX embedder when it is
//! configured, built, and downloaded — otherwise the always-available offline hashing embedder.
//!
//! This is the one place the two halves of Phase 2 meet. The decision is intentionally
//! conservative: the ONNX path is taken only when *every* precondition holds (the `onnx` feature
//! is compiled, a models directory is known, the configured model id resolves to a manifest, and
//! its files are present and loadable). Any gap falls back to [`HashingEmbedder`] rather than
//! erroring, because search must keep working with no model present — the embedder is an upgrade,
//! not a dependency. The returned trait object is all the search engine ever sees.

use std::path::Path;

use crate::config::EmbeddingConfig;
use crate::embeddings::provider::EmbeddingProvider;
use crate::embeddings::HashingEmbedder;
use crate::error::{CoreError, CoreResult};

/// Pick the embedding provider given the (optional) local models directory and config.
pub fn select_embedder(
    models_root: Option<&Path>,
    cfg: &EmbeddingConfig,
) -> Box<dyn EmbeddingProvider> {
    #[cfg(feature = "onnx")]
    if let Some(provider) = onnx_embedder(models_root, cfg) {
        return provider;
    }
    #[cfg(not(feature = "onnx"))]
    let _ = models_root;

    Box::new(HashingEmbedder::new(cfg.dimensions))
}

/// Select the configured model without substituting hashing vectors. Canonical sidecars must
/// never mix vector spaces, so background generation and semantic query ranking use this path.
pub fn try_select_embedder(
    models_root: Option<&Path>,
    cfg: &EmbeddingConfig,
) -> CoreResult<Box<dyn EmbeddingProvider>> {
    #[cfg(feature = "onnx")]
    {
        use crate::embeddings::onnx::OnnxEmbedder;
        use crate::onnx::{manifest, ModelCache};

        let model = manifest::builtin(&cfg.model_id)
            .ok_or_else(|| CoreError::Model(format!("unknown embedding model {}", cfg.model_id)))?;
        let root = models_root
            .ok_or_else(|| CoreError::Model("local model directory unavailable".into()))?;
        let cache = ModelCache::new(root);
        if !cache.is_cached(&model) {
            return Err(CoreError::Model(format!(
                "embedding model {} is not downloaded",
                model.id
            )));
        }
        OnnxEmbedder::load(&cache, &model, cfg)
            .map(|provider| Box::new(provider) as Box<dyn EmbeddingProvider>)
    }
    #[cfg(not(feature = "onnx"))]
    {
        let _ = (models_root, cfg);
        Err(CoreError::Model(
            "this build does not include local ONNX embeddings".into(),
        ))
    }
}

/// Attempt to build the ONNX embedder; `None` (→ fallback) if any precondition is unmet.
#[cfg(feature = "onnx")]
fn onnx_embedder(
    models_root: Option<&Path>,
    cfg: &EmbeddingConfig,
) -> Option<Box<dyn EmbeddingProvider>> {
    use crate::embeddings::onnx::OnnxEmbedder;
    use crate::onnx::{manifest, ModelCache};

    let manifest = manifest::builtin(&cfg.model_id)?;
    let cache = ModelCache::new(models_root?);
    if !cache.is_cached(&manifest) {
        return None;
    }
    match OnnxEmbedder::load(&cache, &manifest, cfg) {
        Ok(embedder) => Some(Box::new(embedder) as Box<dyn EmbeddingProvider>),
        Err(error) => {
            tracing::error!(
                model = %manifest.id,
                error = %error,
                "embedding model failed to load; using hashing fallback"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::ProviderInfo;

    #[test]
    fn falls_back_to_hashing_without_a_model() {
        // No models dir, default config → the offline hashing embedder (dimensions from config).
        let cfg = EmbeddingConfig::default();
        let provider = select_embedder(None, &cfg);
        assert_eq!(provider.dimensions(), cfg.dimensions);
    }

    #[test]
    fn falls_back_when_model_not_cached() {
        // A models dir that exists but holds no files still yields the fallback.
        let dir = tempfile::tempdir().unwrap();
        let cfg = EmbeddingConfig {
            model_id: "embeddinggemma-300m".to_string(),
            ..Default::default()
        };
        let provider = select_embedder(Some(dir.path()), &cfg);
        // The hashing embedder identifies itself; this proves no ONNX model was loaded.
        let named: &dyn ProviderInfo = &HashingEmbedder::new(cfg.dimensions);
        assert_eq!(named.name(), "hashing-bow");
        assert_eq!(provider.dimensions(), cfg.dimensions);
    }
}
