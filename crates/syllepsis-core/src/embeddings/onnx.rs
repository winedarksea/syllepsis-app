//! The model-backed [`EmbeddingProvider`]: Qwen3-Embedding-0.6B on ONNX Runtime (feature `onnx`).
//!
//! This is the Phase-2 redefinition — the embedder now rides the *same* ONNX stack as the LLM
//! ([`onnx`](crate::onnx)). It is one forward pass: tokenize, run the graph, pool the hidden
//! states ([`pooling`](super::pooling)), optionally truncate (Matryoshka), normalize. All the
//! numeric shaping lives in the pure, tested [`pooling`](super::pooling) module; what is here is
//! only the runtime plumbing. The model is *asymmetric*, so [`embed`](OnnxEmbedder::embed) embeds
//! a document raw while [`embed_query`](EmbeddingProvider::embed_query) wraps the text in the
//! manifest's retrieval instruction first. When the `onnx` feature is off, the offline
//! [`HashingEmbedder`](super::HashingEmbedder) is the provider and none of this compiles.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use crate::config::EmbeddingConfig;
use crate::embeddings::pooling::{format_query, matryoshka_embedding, pool};
use crate::embeddings::provider::{EmbeddingProvider, ProviderInfo};
use crate::embeddings::vector::Embedding;
use crate::error::CoreResult;
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::{ModelManifest, PoolingStrategy};
use crate::onnx::session::{map_ort_err, ModelSession};
use crate::onnx::tokenizer::ModelTokenizer;
use crate::onnx::RuntimeDiagnostics;

/// Qwen3-Embedding-0.6B (or any manifest-described embedder) behind the embedding seam.
pub struct OnnxEmbedder {
    // `Session::run` needs `&mut`, but the seam is `&self`; the Mutex makes the provider usable
    // behind a shared `Box<dyn EmbeddingProvider>` while keeping inference serialized per session.
    session: Mutex<Session>,
    tokenizer: ModelTokenizer,
    pooling: PoolingStrategy,
    /// Instruction template for the query side of an asymmetric model; `None` ⇒ symmetric.
    query_instruction: Option<String>,
    /// The model's native output width.
    native_dim: usize,
    /// Matryoshka truncation target (`< native_dim`), or `None` to keep the full vector.
    target_dim: Option<usize>,
    name: String,
    diagnostics: RuntimeDiagnostics,
}

impl OnnxEmbedder {
    /// Load an embedder from already-cached model files. Performs a tiny warm-up inference so a
    /// broken model or shape mismatch fails here, at load, rather than silently as a zero vector
    /// on the first real query.
    pub fn load(
        cache: &ModelCache,
        manifest: &ModelManifest,
        cfg: &EmbeddingConfig,
    ) -> CoreResult<OnnxEmbedder> {
        let weights = manifest
            .weights_file()
            .ok_or_else(|| crate::error::CoreError::Model("manifest has no weights".into()))?;
        let tok_file = manifest
            .tokenizer_file()
            .ok_or_else(|| crate::error::CoreError::Model("manifest has no tokenizer".into()))?;

        let loaded = ModelSession::load(&cache.file_path(manifest, weights), manifest)?;
        let tokenizer = ModelTokenizer::from_file(&cache.file_path(manifest, tok_file))?;

        let target_dim = cfg
            .matryoshka_dims
            .filter(|&d| d > 0 && d < manifest.hidden_size);

        let embedder = OnnxEmbedder {
            session: Mutex::new(loaded.session),
            tokenizer,
            pooling: manifest.pooling.unwrap_or(PoolingStrategy::LastToken),
            query_instruction: manifest.query_instruction.clone(),
            native_dim: manifest.hidden_size,
            target_dim,
            name: manifest.id.clone(),
            diagnostics: loaded.diagnostics,
        };
        // Warm-up: surface load-time failures immediately.
        embedder.run(&format!("{} warmup", manifest.id))?;
        Ok(embedder)
    }

    /// Where and how this model is running, for the Diagnostics view.
    pub fn diagnostics(&self) -> &RuntimeDiagnostics {
        &self.diagnostics
    }

    /// Tokenize, run the graph, pool, truncate, normalize. Empty input ⇒ a zero vector.
    fn run(&self, text: &str) -> CoreResult<Embedding> {
        let ids = self.tokenizer.encode(text, true)?;
        if ids.is_empty() {
            return Ok(Embedding::zeros(self.dimensions()));
        }
        let seq = ids.len();
        let mask: Vec<i64> = vec![1; seq];

        let input_ids = Tensor::from_array((vec![1_i64, seq as i64], ids)).map_err(map_ort_err)?;
        let attention =
            Tensor::from_array((vec![1_i64, seq as i64], mask.clone())).map_err(map_ort_err)?;

        let mut session = self.session.lock().expect("embedding session poisoned");
        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "attention_mask" => attention,
            ])
            .map_err(map_ort_err)?;

        // The base-model export's first output is the [1, seq, hidden] hidden-state tensor.
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(map_ort_err)?;
        let hidden_size = shape.last().copied().unwrap_or(0) as usize;
        let pooled = pool(self.pooling, data, seq, hidden_size, &mask);
        if pooled.is_empty() {
            return Ok(Embedding::zeros(self.dimensions()));
        }
        Ok(matryoshka_embedding(pooled, self.target_dim))
    }
}

impl EmbeddingProvider for OnnxEmbedder {
    fn dimensions(&self) -> usize {
        self.target_dim.unwrap_or(self.native_dim)
    }

    fn embed(&self, text: &str) -> Embedding {
        // The seam is infallible; load-time warm-up makes runtime failure unlikely, and a zero
        // vector is the established "no signal" value (never a panic in a ranking path).
        self.run(text)
            .unwrap_or_else(|_| Embedding::zeros(self.dimensions()))
    }

    fn embed_query(&self, text: &str) -> Embedding {
        match &self.query_instruction {
            Some(instruction) => self
                .run(&format_query(instruction, text))
                .unwrap_or_else(|_| Embedding::zeros(self.dimensions())),
            None => self.embed(text),
        }
    }
}

impl ProviderInfo for OnnxEmbedder {
    fn name(&self) -> &str {
        &self.name
    }
}
