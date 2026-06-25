//! A thin wrapper over the Hugging Face [`tokenizers`] fast tokenizer, shared by the ONNX
//! embedder and the ONNX LLM (feature `onnx`).
//!
//! Local models ship a Hugging Face `tokenizer.json`; loading it here keeps token id handling —
//! to `i64` for the model's `input_ids`, decode generated ids back to text — in one place, and
//! maps the crate's `Box<dyn Error>` tokenizer failures onto [`CoreError::Model`]. Special-token
//! ids (the chat sentinels the LLM stops on) are resolved by name so a tokenizer revision that
//! renumbers them does not silently break generation.

use std::path::Path;

use tokenizers::Tokenizer;

use crate::error::{CoreError, CoreResult};

/// A loaded tokenizer for one model.
pub struct ModelTokenizer {
    inner: Tokenizer,
}

impl ModelTokenizer {
    /// Load a `tokenizer.json` from disk.
    pub fn from_file(path: &Path) -> CoreResult<ModelTokenizer> {
        let inner = Tokenizer::from_file(path)
            .map_err(|e| CoreError::Model(format!("tokenizer load failed: {e}")))?;
        Ok(ModelTokenizer { inner })
    }

    /// Encode text to token ids as `i64` (the dtype ONNX `input_ids` expect). `add_special` adds
    /// the model's special tokens (e.g. BOS) — off for chat prompts that already carry their own
    /// ChatML sentinels, on for raw embedding text.
    pub fn encode(&self, text: &str, add_special: bool) -> CoreResult<Vec<i64>> {
        let encoding = self
            .inner
            .encode(text, add_special)
            .map_err(|e| CoreError::Model(format!("tokenize failed: {e}")))?;
        Ok(encoding.get_ids().iter().map(|&id| id as i64).collect())
    }

    /// Decode generated token ids back to text, optionally dropping special tokens.
    pub fn decode(&self, ids: &[u32], skip_special: bool) -> CoreResult<String> {
        self.inner
            .decode(ids, skip_special)
            .map_err(|e| CoreError::Model(format!("detokenize failed: {e}")))
    }

    pub fn encode_u32(&self, text: &str, add_special: bool) -> CoreResult<Vec<u32>> {
        let encoding = self
            .inner
            .encode(text, add_special)
            .map_err(|e| CoreError::Model(format!("tokenize failed: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    /// The id of a named special token (e.g. `<|im_end|>`), if the tokenizer defines it.
    pub fn token_id(&self, token: &str) -> Option<u32> {
        self.inner.token_to_id(token)
    }
}
