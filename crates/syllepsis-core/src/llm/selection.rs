//! Choosing the in-process LLM provider for a book.
//!
//! Mirrors [`embeddings::selection`](crate::embeddings::selection). Cloud providers are *not*
//! constructed here: the shell owns cloud/local-server HTTP execution with keychain credentials and
//! only their results re-enter the core. From core's perspective, the bundled local model is the
//! only in-process generation provider; missing local model setup is a visible error.

use std::path::Path;

use crate::config::LlmConfig;
use crate::error::{CoreError, CoreResult};
use crate::llm::LlmProvider;

/// The config value of [`LlmConfig::provider`] that selects the bundled local ONNX model.
pub const LOCAL_PROVIDER: &str = "local";

/// Pick the in-process LLM provider given the (optional) local models directory and config.
pub fn select_llm_provider(
    models_root: Option<&Path>,
    cfg: &LlmConfig,
) -> CoreResult<Box<dyn LlmProvider>> {
    if !cfg.enabled {
        return Err(CoreError::Llm("LLM features are disabled".to_string()));
    }
    if cfg.provider != LOCAL_PROVIDER {
        return Err(CoreError::Llm(format!(
            "provider {} is not an in-process local LLM; use the cloud/server LLM path",
            cfg.provider
        )));
    }

    onnx_provider(models_root, cfg)
}

/// Attempt to build the ONNX LLM provider.
#[cfg(feature = "onnx")]
fn onnx_provider(models_root: Option<&Path>, cfg: &LlmConfig) -> CoreResult<Box<dyn LlmProvider>> {
    use crate::llm::onnx::OnnxLlmProvider;
    use crate::onnx::{manifest, ModelCache};

    let manifest = manifest::builtin(&cfg.local_model).ok_or_else(|| {
        CoreError::Llm(format!(
            "unknown local LLM model manifest: {}",
            cfg.local_model
        ))
    })?;
    let models_root = models_root.ok_or_else(|| {
        CoreError::Llm("no local LLM model cache directory is configured".to_string())
    })?;
    let cache = ModelCache::new(models_root);
    if !cache.is_cached(&manifest) {
        let missing_files = cache
            .missing_files(&manifest)
            .into_iter()
            .map(|file| file.file_name().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CoreError::Llm(format!(
            "local LLM model {} is not ready in {}; missing or wrong-size files: {}",
            manifest.id,
            cache.root().display(),
            missing_files
        )));
    }
    tracing::info!(
        model = %manifest.id,
        cache_root = %cache.root().display(),
        "local LLM cache ready; loading provider"
    );
    OnnxLlmProvider::load(&cache, &manifest, cfg.max_new_tokens)
        .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
        .map_err(|error| {
            CoreError::Llm(format!(
                "failed to load local LLM model {} from {}: {}",
                manifest.id,
                cache.root().display(),
                error
            ))
        })
}

#[cfg(not(feature = "onnx"))]
fn onnx_provider(_models_root: Option<&Path>, cfg: &LlmConfig) -> CoreResult<Box<dyn LlmProvider>> {
    Err(CoreError::Llm(format!(
        "local LLM model {} cannot run because this build was compiled without ONNX support",
        cfg.local_model
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_when_provider_is_not_local() {
        let cfg = LlmConfig {
            provider: "anthropic".to_string(),
            ..Default::default()
        };
        let err = match select_llm_provider(None, &cfg) {
            Ok(_) => panic!("non-local provider should not build an in-process LLM provider"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("cloud/server LLM path"));
    }

    #[test]
    fn errors_when_local_requested_but_model_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = LlmConfig {
            provider: LOCAL_PROVIDER.to_string(),
            ..Default::default()
        };
        let err = match select_llm_provider(Some(dir.path()), &cfg) {
            Ok(_) => panic!("missing local model should not build an LLM provider"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("local LLM model"));
    }
}
