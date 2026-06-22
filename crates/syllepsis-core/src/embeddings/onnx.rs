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

use std::borrow::Cow;
use std::sync::Mutex;

use ndarray::Array4;
use ort::session::Session;
use ort::session::SessionInputValue;
use ort::value::{Outlet, Tensor};
use serde::Deserialize;

use crate::config::EmbeddingConfig;
use crate::embeddings::pooling::{format_query, matryoshka_embedding, pool};
use crate::embeddings::provider::{EmbeddingProvider, ProviderInfo};
use crate::embeddings::vector::Embedding;
use crate::error::CoreResult;
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::{FileRole, ModelManifest, PoolingStrategy};
use crate::onnx::session::{map_ort_err, ModelSession};
use crate::onnx::tokenizer::ModelTokenizer;
use crate::onnx::RuntimeDiagnostics;

/// Qwen3-Embedding-0.6B (or any manifest-described embedder) behind the embedding seam.
pub struct OnnxEmbedder {
    // `Session::run` needs `&mut`, but the seam is `&self`; the Mutex makes the provider usable
    // behind a shared `Box<dyn EmbeddingProvider>` while keeping inference serialized per session.
    session: Mutex<Session>,
    tokenizer: ModelTokenizer,
    input_ids_input_name: String,
    attention_mask_input_name: String,
    position_ids_input_name: Option<String>,
    token_type_ids_input_name: Option<String>,
    cache_position_input_name: Option<String>,
    past_input_names: Vec<(String, String)>,
    cache_config: Option<EmbeddingCacheConfig>,
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
        let config_file = manifest.files.iter().find(|f| f.role == FileRole::Config);

        let loaded = ModelSession::load(&cache.file_path(manifest, weights), manifest)?;
        let input_names = outlet_names(loaded.session.inputs());
        let tokenizer = ModelTokenizer::from_file(&cache.file_path(manifest, tok_file))?;
        let past_input_names = available_cache_names(
            &input_names,
            &["past_key_values.{i}.key"],
            &["past_key_values.{i}.value"],
        );
        let cache_config = if past_input_names.is_empty() {
            None
        } else {
            let config_file = config_file.ok_or_else(|| {
                crate::error::CoreError::Model(
                    "embedder graph has cache inputs but manifest has no config.json".into(),
                )
            })?;
            let config_text = std::fs::read_to_string(cache.file_path(manifest, config_file))?;
            Some(parse_embedding_cache_config(&config_text)?)
        };

        let target_dim = cfg
            .matryoshka_dims
            .filter(|&d| d > 0 && d < manifest.hidden_size);

        let embedder = OnnxEmbedder {
            session: Mutex::new(loaded.session),
            tokenizer,
            input_ids_input_name: required_name(
                &input_names,
                &["input_ids"],
                "embedder input ids",
            )?,
            attention_mask_input_name: required_name(
                &input_names,
                &["attention_mask"],
                "embedder attention mask",
            )?,
            position_ids_input_name: optional_name(&input_names, &["position_ids"]),
            token_type_ids_input_name: optional_name(&input_names, &["token_type_ids"]),
            cache_position_input_name: optional_name(&input_names, &["cache_position"]),
            past_input_names,
            cache_config,
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
        let mut inputs: Vec<(Cow<str>, SessionInputValue)> = vec![
            (self.input_ids_input_name.clone().into(), input_ids.into()),
            (
                self.attention_mask_input_name.clone().into(),
                attention.into(),
            ),
        ];
        if let Some(name) = &self.position_ids_input_name {
            let positions: Vec<i64> = (0..seq).map(|position| position as i64).collect();
            inputs.push((
                name.clone().into(),
                Tensor::from_array((vec![1_i64, seq as i64], positions))
                    .map_err(map_ort_err)?
                    .into(),
            ));
        }
        if let Some(name) = &self.token_type_ids_input_name {
            inputs.push((
                name.clone().into(),
                Tensor::from_array((vec![1_i64, seq as i64], vec![0_i64; seq]))
                    .map_err(map_ort_err)?
                    .into(),
            ));
        }
        if let Some(name) = &self.cache_position_input_name {
            let positions: Vec<i64> = (0..seq).map(|position| position as i64).collect();
            inputs.push((
                name.clone().into(),
                Tensor::from_array((vec![seq as i64], positions))
                    .map_err(map_ort_err)?
                    .into(),
            ));
        }
        if let Some(cache_config) = &self.cache_config {
            for (key_name, value_name) in &self.past_input_names {
                let empty_cache = Array4::<f32>::zeros((
                    1,
                    cache_config.num_key_value_heads,
                    0,
                    cache_config.head_dim(),
                ));
                inputs.push((
                    key_name.clone().into(),
                    Tensor::from_array(empty_cache.clone())
                        .map_err(map_ort_err)?
                        .into(),
                ));
                inputs.push((
                    value_name.clone().into(),
                    Tensor::from_array(empty_cache).map_err(map_ort_err)?.into(),
                ));
            }
        }

        let mut session = self.session.lock().expect("embedding session poisoned");
        let outputs = session.run(inputs).map_err(map_ort_err)?;

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

fn outlet_names(outlets: &[Outlet]) -> Vec<String> {
    outlets
        .iter()
        .map(|outlet| outlet.name().to_string())
        .collect()
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingCacheConfig {
    num_key_value_heads: usize,
    num_attention_heads: usize,
    hidden_size: usize,
    #[serde(default)]
    head_dim: Option<usize>,
}

impl EmbeddingCacheConfig {
    fn head_dim(&self) -> usize {
        self.head_dim
            .unwrap_or_else(|| self.hidden_size / self.num_attention_heads.max(1))
    }
}

fn parse_embedding_cache_config(config_text: &str) -> CoreResult<EmbeddingCacheConfig> {
    serde_json::from_str(config_text)
        .map_err(|e| crate::error::CoreError::Model(format!("config.json parse: {e}")))
}

fn required_name(names: &[String], candidates: &[&str], label: &str) -> CoreResult<String> {
    optional_name(names, candidates).ok_or_else(|| {
        crate::error::CoreError::Model(format!(
            "missing {label}; tried {candidates:?}; available names: {names:?}"
        ))
    })
}

fn optional_name(names: &[String], candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|candidate| names.iter().any(|name| name == **candidate))
        .map(|candidate| (*candidate).to_string())
}

fn available_cache_names(
    names: &[String],
    key_patterns: &[&str],
    value_patterns: &[&str],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for layer in 0.. {
        let key_name = optional_pattern_name(names, key_patterns, layer);
        let value_name = optional_pattern_name(names, value_patterns, layer);
        match (key_name, value_name) {
            (Some(key_name), Some(value_name)) => out.push((key_name, value_name)),
            (None, None) => break,
            _ => break,
        }
    }
    out
}

fn optional_pattern_name(names: &[String], patterns: &[&str], layer: usize) -> Option<String> {
    let candidates: Vec<String> = patterns
        .iter()
        .map(|pattern| pattern.replace("{i}", &layer.to_string()))
        .collect();
    candidates
        .into_iter()
        .find(|candidate| names.iter().any(|name| name == candidate))
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
