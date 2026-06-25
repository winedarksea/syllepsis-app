#![cfg(feature = "onnx")]

use std::path::PathBuf;

use syllepsis_core::config::ModelRef;
use syllepsis_core::config::{EmbeddingConfig, LlmConfig};
use syllepsis_core::embeddings::{EmbeddingProvider, OnnxEmbedder};
use syllepsis_core::llm::{LlmProvider, LlmRequest, LlmTask, OnnxLlmProvider};
use syllepsis_core::onnx::{builtin, ModelCache, BUNDLED_LLM_ID, EMBEDDINGGEMMA_ID};

fn live_cache() -> ModelCache {
    let root = std::env::var("SYLLEPSIS_MODEL_CACHE").expect(
        "set SYLLEPSIS_MODEL_CACHE to the app-data models directory before running ignored live tests",
    );
    ModelCache::new(PathBuf::from(root))
}

fn require_cached(cache: &ModelCache, model_id: &str) {
    let manifest = builtin(model_id).expect("built-in model manifest exists");
    if !cache.is_cached(&manifest) {
        let missing: Vec<&str> = cache
            .missing_files(&manifest)
            .into_iter()
            .map(|f| f.repo_path.as_str())
            .collect();
        panic!("model {model_id} is not fully cached; missing: {missing:?}");
    }
}

#[test]
#[ignore = "requires SYLLEPSIS_MODEL_CACHE with EmbeddingGemma files downloaded"]
fn embeddinggemma_onnx_runs_real_inference() {
    let cache = live_cache();
    require_cached(&cache, EMBEDDINGGEMMA_ID);
    let manifest = builtin(EMBEDDINGGEMMA_ID).unwrap();
    let embedder = OnnxEmbedder::load(&cache, &manifest, &EmbeddingConfig::default()).unwrap();

    let document = embedder.embed("Local-first notes need reliable vector search.");
    let query = embedder.embed_query("vector search for local notes");

    assert_eq!(document.len(), 256);
    assert_eq!(query.len(), 256);
    assert!(document.magnitude() > 0.99);
    assert!(query.magnitude() > 0.99);
    assert!(document.cosine_similarity(&query).is_finite());
}

#[test]
#[ignore = "requires SYLLEPSIS_MODEL_CACHE with Gemma 4 E2B files downloaded"]
fn gemma_4_onnx_runs_real_completion() {
    let cache = live_cache();
    require_cached(&cache, BUNDLED_LLM_ID);
    let manifest = builtin(BUNDLED_LLM_ID).unwrap();
    let cfg = LlmConfig {
        max_new_tokens: 32,
        ..Default::default()
    };
    let provider = OnnxLlmProvider::load(&cache, &manifest, cfg.max_new_tokens).unwrap();
    let response = provider
        .complete(&LlmRequest {
            task: LlmTask::Summarize,
            model_ref: ModelRef::new("local", BUNDLED_LLM_ID),
            system: "Answer in one short sentence.".to_string(),
            user: "Summarize: Syllepsis is a local-first note app for organizing books of markdown notes.".to_string(),
        })
        .unwrap();

    assert!(!response.text.trim().is_empty());
    assert!(response.text.split_whitespace().count() <= 80);
}
