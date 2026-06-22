//! The bundled local LLM: Gemma 4 E2B on ONNX Runtime (feature `onnx`) — Phase 3.
//!
//! This is the live counterpart to the [`OfflineLlmProvider`](super::OfflineLlmProvider), behind
//! the same [`LlmProvider`] seam, so enabling it changes nothing above the provider. It runs the
//! split decoder ONNX export used by Gemma 4: token ids pass through `embed_tokens`, then the
//! autoregressive loop feeds embeddings into `decoder_model_merged`, threading the model's
//! key/value cache (`present.*` outputs → `past_key_values.*` inputs). The prompt is shaped by the
//! pure [`chat`](super::chat) helpers (Gemma turn format); architecture dimensions are read from
//! `config.json`, never hard-coded, so a manifest swap to a later compatible release is config.
//! Greedy decoding is deterministic, which keeps the proposal flow reproducible; sampling is a
//! later knob.

use std::borrow::Cow;
use std::sync::Mutex;

use ort::session::{Session, SessionInputValue};
use ort::value::{Outlet, Tensor};
use serde::Deserialize;

use crate::error::{CoreError, CoreResult};
use crate::llm::chat::{build_prompt, strip_thinking, STOP_TOKEN};
use crate::llm::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::onnx::cache::ModelCache;
use crate::onnx::manifest::{FileRole, ModelManifest};
use crate::onnx::session::{map_ort_err, ModelSession};
use crate::onnx::tokenizer::ModelTokenizer;
use crate::onnx::RuntimeDiagnostics;

/// The architecture parameters the decode loop needs, read from the model's `config.json`.
#[derive(Debug, Clone, Deserialize)]
struct LlmModelConfig {
    num_hidden_layers: usize,
    num_key_value_heads: usize,
    num_attention_heads: usize,
    hidden_size: usize,
    /// Qwen3 sets head_dim explicitly (decoupled from hidden_size/heads); older configs omit it.
    #[serde(default)]
    head_dim: Option<usize>,
}

impl LlmModelConfig {
    /// Per-head dimension of the KV cache: the explicit `head_dim`, else `hidden_size / heads`.
    fn head_dim(&self) -> usize {
        self.head_dim
            .unwrap_or_else(|| self.hidden_size / self.num_attention_heads.max(1))
    }
}

/// Gemma 4 E2B (or any manifest-described causal LLM with the decoder-with-past I/O) behind the LLM seam.
pub struct OnnxLlmProvider {
    // `Session::run` needs `&mut`; Mutexes let the provider sit behind a shared trait object while
    // serializing the inherently sequential decode.
    token_embeddings_session: Option<TokenEmbeddingsSession>,
    decoder_session: Mutex<Session>,
    tokenizer: ModelTokenizer,
    config: LlmModelConfig,
    decoder_token_input_name: String,
    decoder_cache_position_input_name: Option<String>,
    decoder_past_input_names: Vec<(String, String)>,
    decoder_present_output_names: Vec<(String, String)>,
    logits_output_name: String,
    /// Token ids that end generation (`<|im_end|>`, and `<|endoftext|>` if present).
    eos_ids: Vec<i64>,
    /// Upper bound on generated tokens per call.
    max_new_tokens: usize,
    name: String,
    diagnostics: RuntimeDiagnostics,
}

impl OnnxLlmProvider {
    /// Load the bundled LLM from already-cached files.
    pub fn load(
        cache: &ModelCache,
        manifest: &ModelManifest,
        max_new_tokens: usize,
    ) -> CoreResult<OnnxLlmProvider> {
        let decoder_file = manifest
            .llm_decoder_graph_file()
            .ok_or_else(|| CoreError::Model("manifest has no LLM decoder graph".into()))?;
        let tok_file = manifest
            .tokenizer_file()
            .ok_or_else(|| CoreError::Model("manifest has no tokenizer".into()))?;
        let config_file = manifest
            .files
            .iter()
            .find(|f| f.role == FileRole::Config)
            .ok_or_else(|| CoreError::Model("manifest has no config.json".into()))?;

        let decoder_loaded =
            ModelSession::load(&cache.file_path(manifest, decoder_file), manifest)?;
        let decoder_input_names = outlet_names(decoder_loaded.session.inputs());
        let decoder_output_names = outlet_names(decoder_loaded.session.outputs());
        let token_embeddings_session = manifest
            .token_embeddings_file()
            .map(|file| ModelSession::load(&cache.file_path(manifest, file), manifest))
            .transpose()?
            .map(TokenEmbeddingsSession::from_loaded)
            .transpose()?;
        let tokenizer = ModelTokenizer::from_file(&cache.file_path(manifest, tok_file))?;

        let config_text = std::fs::read_to_string(cache.file_path(manifest, config_file))?;
        let config: LlmModelConfig = serde_json::from_str(&config_text)
            .map_err(|e| CoreError::Model(format!("config.json parse: {e}")))?;

        // Resolve stop tokens by name so a renumbered vocab can't silently disable EOS.
        let eos_ids: Vec<i64> = [STOP_TOKEN, "<eos>", "<end_of_sequence>"]
            .iter()
            .filter_map(|t| tokenizer.token_id(t))
            .map(|id| id as i64)
            .collect();
        if eos_ids.is_empty() {
            return Err(CoreError::Model("tokenizer defines no stop token".into()));
        }
        let layers = config.num_hidden_layers;
        let decoder_token_input_name = if token_embeddings_session.is_some() {
            required_name(
                &decoder_input_names,
                &["inputs_embeds"],
                "decoder token embedding input",
            )?
        } else {
            required_name(
                &decoder_input_names,
                &["input_ids"],
                "decoder token id input",
            )?
        };

        Ok(OnnxLlmProvider {
            token_embeddings_session,
            decoder_session: Mutex::new(decoder_loaded.session),
            tokenizer,
            config,
            decoder_token_input_name,
            decoder_cache_position_input_name: optional_name(
                &decoder_input_names,
                &["cache_position"],
            ),
            decoder_past_input_names: cache_names(
                &decoder_input_names,
                layers,
                &["past_key_values.{i}.key"],
                &["past_key_values.{i}.value"],
                "decoder cache input",
            )?,
            decoder_present_output_names: cache_names(
                &decoder_output_names,
                layers,
                &["present.{i}.key"],
                &["present.{i}.value"],
                "decoder cache output",
            )?,
            logits_output_name: required_name(
                &decoder_output_names,
                &["logits"],
                "decoder logits output",
            )?,
            eos_ids,
            max_new_tokens: max_new_tokens.max(1),
            name: crate::llm::selection::LOCAL_PROVIDER.to_string(),
            diagnostics: decoder_loaded.diagnostics,
        })
    }

    /// Where and how this model is running, for the Diagnostics view.
    pub fn diagnostics(&self) -> &RuntimeDiagnostics {
        &self.diagnostics
    }

    /// Greedy autoregressive decode with a threaded KV cache. Returns the generated token ids
    /// (excluding the stop token).
    fn generate(&self, prompt_ids: Vec<i64>) -> CoreResult<Vec<u32>> {
        let layers = self.config.num_hidden_layers;
        let kv_heads = self.config.num_key_value_heads as i64;
        let head_dim = self.config.head_dim() as i64;

        // Per-layer (key, value) cache contents, flat row-major; starts empty (past_len = 0).
        let mut past: Vec<(Vec<f32>, Vec<f32>)> = vec![(Vec::new(), Vec::new()); layers];
        let mut past_len: usize = 0;
        let mut current: Vec<i64> = prompt_ids;
        let mut generated: Vec<u32> = Vec::new();

        let mut decoder_session = self
            .decoder_session
            .lock()
            .expect("llm decoder session poisoned");

        for _ in 0..self.max_new_tokens {
            let seq = current.len();
            let total = past_len + seq;
            let attention_mask: Vec<i64> = vec![1; total];
            let position_ids: Vec<i64> = (past_len..total).map(|p| p as i64).collect();

            let mut inputs: Vec<(Cow<str>, SessionInputValue)> = Vec::with_capacity(4 + layers * 2);
            match self.input_ids_or_embeddings(&current, seq)? {
                DecoderTokenInput::InputIds(ids) => inputs.push((
                    self.decoder_token_input_name.clone().into(),
                    Tensor::from_array((vec![1_i64, seq as i64], ids))
                        .map_err(map_ort_err)?
                        .into(),
                )),
                DecoderTokenInput::InputsEmbeds { data, hidden_size } => inputs.push((
                    self.decoder_token_input_name.clone().into(),
                    Tensor::from_array((vec![1_i64, seq as i64, hidden_size as i64], data))
                        .map_err(map_ort_err)?
                        .into(),
                )),
            }
            inputs.push((
                "attention_mask".into(),
                Tensor::from_array((vec![1_i64, total as i64], attention_mask))
                    .map_err(map_ort_err)?
                    .into(),
            ));
            inputs.push((
                "position_ids".into(),
                Tensor::from_array((vec![1_i64, seq as i64], position_ids))
                    .map_err(map_ort_err)?
                    .into(),
            ));
            if let Some(name) = &self.decoder_cache_position_input_name {
                inputs.push((
                    name.clone().into(),
                    Tensor::from_array((
                        vec![seq as i64],
                        (past_len..total).map(|p| p as i64).collect::<Vec<i64>>(),
                    ))
                    .map_err(map_ort_err)?
                    .into(),
                ));
            }
            for (i, (key, value)) in past.iter().enumerate() {
                let shape = vec![1_i64, kv_heads, past_len as i64, head_dim];
                let (key_name, value_name) = &self.decoder_past_input_names[i];
                inputs.push((
                    key_name.clone().into(),
                    Tensor::from_array((shape.clone(), key.clone()))
                        .map_err(map_ort_err)?
                        .into(),
                ));
                inputs.push((
                    value_name.clone().into(),
                    Tensor::from_array((shape, value.clone()))
                        .map_err(map_ort_err)?
                        .into(),
                ));
            }

            let outputs = decoder_session.run(inputs).map_err(map_ort_err)?;

            // Next token = argmax of the final position's logits ([1, seq, vocab]).
            let logits_output = outputs.get(&self.logits_output_name).ok_or_else(|| {
                CoreError::Model(format!(
                    "missing decoder output {}",
                    self.logits_output_name
                ))
            })?;
            let (logits_shape, logits) = logits_output
                .try_extract_tensor::<f32>()
                .map_err(map_ort_err)?;
            let vocab = logits_shape.last().copied().unwrap_or(0) as usize;
            if vocab == 0 {
                return Err(CoreError::Model("logits had zero vocab dim".into()));
            }
            let last = (seq - 1) * vocab;
            let next_id = argmax(&logits[last..last + vocab]) as i64;

            // Capture the refreshed cache before the borrowed `outputs` is dropped.
            let mut next_past: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(layers);
            for (key_name, value_name) in &self.decoder_present_output_names {
                let key_output = outputs.get(key_name).ok_or_else(|| {
                    CoreError::Model(format!("missing decoder output {key_name}"))
                })?;
                let value_output = outputs.get(value_name).ok_or_else(|| {
                    CoreError::Model(format!("missing decoder output {value_name}"))
                })?;
                let (_, key) = key_output
                    .try_extract_tensor::<f32>()
                    .map_err(map_ort_err)?;
                let (_, value) = value_output
                    .try_extract_tensor::<f32>()
                    .map_err(map_ort_err)?;
                next_past.push((key.to_vec(), value.to_vec()));
            }
            drop(outputs);

            past = next_past;
            past_len = total;

            if self.eos_ids.contains(&next_id) {
                break;
            }
            generated.push(next_id as u32);
            current = vec![next_id];
        }
        Ok(generated)
    }

    /// The decoder either accepts token ids directly (single-session exports) or embeddings from a
    /// separate token-embedding session (Gemma 4 split text path).
    fn input_ids_or_embeddings(&self, ids: &[i64], seq: usize) -> CoreResult<DecoderTokenInput> {
        let Some(token_embeddings) = &self.token_embeddings_session else {
            return Ok(DecoderTokenInput::InputIds(ids.to_vec()));
        };

        let input =
            Tensor::from_array((vec![1_i64, seq as i64], ids.to_vec())).map_err(map_ort_err)?;
        let mut session = token_embeddings
            .session
            .lock()
            .expect("llm token embeddings session poisoned");
        let outputs = session
            .run(ort::inputs![token_embeddings.input_name.as_str() => input])
            .map_err(map_ort_err)?;
        let embeddings_output = outputs.get(&token_embeddings.output_name).ok_or_else(|| {
            CoreError::Model(format!(
                "missing token embeddings output {}",
                token_embeddings.output_name
            ))
        })?;
        let (shape, data) = embeddings_output
            .try_extract_tensor::<f32>()
            .map_err(map_ort_err)?;
        let hidden_size = shape
            .last()
            .copied()
            .unwrap_or(self.config.hidden_size as i64) as usize;
        if hidden_size == 0 {
            return Err(CoreError::Model(
                "token embeddings had zero hidden dim".into(),
            ));
        }
        Ok(DecoderTokenInput::InputsEmbeds {
            data: data.to_vec(),
            hidden_size,
        })
    }
}

enum DecoderTokenInput {
    InputIds(Vec<i64>),
    InputsEmbeds { data: Vec<f32>, hidden_size: usize },
}

struct TokenEmbeddingsSession {
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
}

impl TokenEmbeddingsSession {
    fn from_loaded(loaded: ModelSession) -> CoreResult<TokenEmbeddingsSession> {
        let input_names = outlet_names(loaded.session.inputs());
        let output_names = outlet_names(loaded.session.outputs());
        Ok(TokenEmbeddingsSession {
            session: Mutex::new(loaded.session),
            input_name: required_name(&input_names, &["input_ids"], "token embeddings input")?,
            output_name: output_names
                .first()
                .cloned()
                .ok_or_else(|| CoreError::Model("token embeddings graph has no outputs".into()))?,
        })
    }
}

fn outlet_names(outlets: &[Outlet]) -> Vec<String> {
    outlets.iter().map(|o| o.name().to_string()).collect()
}

fn cache_names(
    names: &[String],
    layers: usize,
    key_patterns: &[&str],
    value_patterns: &[&str],
    label: &str,
) -> CoreResult<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(layers);
    for layer in 0..layers {
        out.push((
            required_pattern_name(names, key_patterns, layer, label)?,
            required_pattern_name(names, value_patterns, layer, label)?,
        ));
    }
    Ok(out)
}

fn required_pattern_name(
    names: &[String],
    patterns: &[&str],
    layer: usize,
    label: &str,
) -> CoreResult<String> {
    let candidates: Vec<String> = patterns
        .iter()
        .map(|pattern| pattern.replace("{i}", &layer.to_string()))
        .collect();
    optional_name_owned(names, &candidates).ok_or_else(|| {
        CoreError::Model(format!(
            "missing {label} for layer {layer}; tried {candidates:?}; available names: {names:?}"
        ))
    })
}

fn required_name(names: &[String], candidates: &[&str], label: &str) -> CoreResult<String> {
    optional_name(names, candidates).ok_or_else(|| {
        CoreError::Model(format!(
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

fn optional_name_owned(names: &[String], candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .find(|candidate| names.iter().any(|name| name == *candidate))
        .cloned()
}

impl LlmProvider for OnnxLlmProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_live(&self) -> bool {
        true
    }

    fn complete(&self, request: &LlmRequest) -> CoreResult<LlmResponse> {
        let prompt = build_prompt(&request.system, &request.user);
        // ChatML carries its own role sentinels as text; don't let the tokenizer add more.
        let prompt_ids = self.tokenizer.encode(&prompt, false)?;
        let generated = self.generate(prompt_ids)?;
        let raw = self.tokenizer.decode(&generated, true)?;
        Ok(LlmResponse {
            text: strip_thinking(&raw),
        })
    }
}

/// Index of the maximum element (first on ties). Empty slice ⇒ 0.
fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .fold((0usize, f32::NEG_INFINITY), |(best_i, best_v), (i, &v)| {
            if v > best_v {
                (i, v)
            } else {
                (best_i, best_v)
            }
        })
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argmax_picks_the_largest_first_on_ties() {
        assert_eq!(argmax(&[0.1, 0.9, 0.3]), 1);
        assert_eq!(argmax(&[1.0, 1.0, 0.5]), 0);
        assert_eq!(argmax(&[]), 0);
    }

    #[test]
    fn head_dim_falls_back_to_hidden_over_heads() {
        let c = LlmModelConfig {
            num_hidden_layers: 28,
            num_key_value_heads: 8,
            num_attention_heads: 16,
            hidden_size: 2048,
            head_dim: None,
        };
        assert_eq!(c.head_dim(), 128);
        let explicit = LlmModelConfig {
            head_dim: Some(128),
            hidden_size: 1024,
            ..c
        };
        assert_eq!(explicit.head_dim(), 128);
    }
}
