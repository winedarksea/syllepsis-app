//! Choosing the LLM provider for a book: the bundled ONNX model when `provider = "local"` and it
//! is built and downloaded — otherwise the offline heuristic provider.
//!
//! Mirrors [`embeddings::selection`](crate::embeddings::selection). Cloud providers are *not*
//! constructed here: the shell owns cloud/local-server HTTP execution with keychain credentials and
//! only their results re-enter the core. From core's perspective, the live local model and the
//! offline heuristic are the only in-process providers. Anything other than a present, loadable
//! local model falls back to [`OfflineLlmProvider`] so the proposal flow always works.

use std::path::Path;

use crate::config::LlmConfig;
use crate::llm::provider::LlmProvider;
use crate::llm::OfflineLlmProvider;

/// The config value of [`LlmConfig::provider`] that selects the bundled local ONNX model.
pub const LOCAL_PROVIDER: &str = "local";

/// Pick the in-process LLM provider given the (optional) local models directory and config.
pub fn select_llm_provider(models_root: Option<&Path>, cfg: &LlmConfig) -> Box<dyn LlmProvider> {
    if !cfg.enabled {
        return Box::new(OfflineLlmProvider::new());
    }
    #[cfg(feature = "onnx")]
    if cfg.provider == LOCAL_PROVIDER {
        if let Some(provider) = onnx_provider(models_root, cfg) {
            return provider;
        }
    }
    #[cfg(not(feature = "onnx"))]
    {
        let _ = models_root;
        let _ = cfg;
    }

    Box::new(OfflineLlmProvider::new())
}

/// Attempt to build the ONNX LLM provider; `None` (→ fallback) if any precondition is unmet.
#[cfg(feature = "onnx")]
fn onnx_provider(models_root: Option<&Path>, cfg: &LlmConfig) -> Option<Box<dyn LlmProvider>> {
    use crate::llm::onnx::OnnxLlmProvider;
    use crate::onnx::{manifest, ModelCache};

    let manifest = manifest::builtin(&cfg.local_model)?;
    let cache = ModelCache::new(models_root?);
    if !cache.is_cached(&manifest) {
        return None;
    }
    OnnxLlmProvider::load(&cache, &manifest, cfg.max_new_tokens)
        .ok()
        .map(|p| Box::new(p) as Box<dyn LlmProvider>)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_when_provider_is_not_local() {
        let cfg = LlmConfig::default(); // provider = "offline"
        let provider = select_llm_provider(None, &cfg);
        assert_eq!(provider.name(), "offline");
        assert!(!provider.is_live());
    }

    #[test]
    fn offline_when_local_requested_but_model_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = LlmConfig {
            provider: LOCAL_PROVIDER.to_string(),
            ..Default::default()
        };
        // Model not downloaded into the dir → fall back to offline rather than erroring.
        let provider = select_llm_provider(Some(dir.path()), &cfg);
        assert_eq!(provider.name(), "offline");
    }
}
