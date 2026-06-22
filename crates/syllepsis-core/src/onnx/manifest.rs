//! The model manifest: the single config-driven description of a downloadable ONNX model that
//! the whole runtime pattern keys off.
//!
//! One manifest names everything needed to fetch and run a model — its Hugging Face repo, the
//! exact files (graphs, external data, tokenizer, configs) with optional sha256s, its quantization, native
//! width, context length, RAM floor, preferred execution providers, and (for embedders) how to
//! pool and prompt. Adding a model — the optional larger LLM, a different embedder — is a new
//! manifest entry, never a code change, which is what lets the LLM and the embedder share this
//! exact machinery (llm-ai-features.md, "the same config-driven model manifest"). The built-in
//! registry below ships the two Qwen3-family models the POC bundles.

use serde::{Deserialize, Serialize};

use crate::onnx::execution_provider::ExecutionProvider;

/// What a model is for. The runtime is identical; the consuming seam differs (embeddings vs LLM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// Produces dense vectors behind [`crate::embeddings::EmbeddingProvider`].
    Embedding,
    /// Generates text behind [`crate::llm::LlmProvider`].
    Llm,
}

/// The weight precision an export is quantized to. Affects download size and the RAM floor; the
/// graph's contrib ops (e.g. `MatMulNBits` for int4) are what tie the export to ONNX Runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantization {
    Fp32,
    Fp16,
    Int8,
    /// 4-bit weights (`MatMulNBits`) with fp32 activations and I/O — small download, and the
    /// decode/pooling math stays in `f32`. The bundled-LLM default.
    Q4,
    /// 4-bit weights with fp16 activations and I/O; smallest, but requires half-precision I/O.
    Q4F16,
}

/// How a transformer's per-token hidden states are reduced to one sentence vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolingStrategy {
    /// Mean over all non-padding token vectors (bge-style, symmetric models).
    Mean,
    /// The last non-padding token's vector (Qwen3-Embedding, a causal model — the final
    /// position has attended to the whole sequence).
    LastToken,
    /// The `[CLS]` token's vector (BERT-style).
    Cls,
}

/// The job a file in the repo plays, so the cache and the session loader can find the weights,
/// the tokenizer, etc. without hard-coding filenames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// A single `.onnx` graph. Embedders usually use this role.
    Weights,
    /// Token-id to embedding lookup graph for split causal LLM exports.
    TokenEmbeddings,
    /// Decoder graph for split causal LLM exports.
    Decoder,
    /// External weight data (`.onnx_data`) referenced by a graph too large for one file.
    WeightsData,
    /// `tokenizer.json` (Hugging Face fast tokenizer).
    Tokenizer,
    /// `tokenizer_config.json` — carries the chat template for LLMs.
    TokenizerConfig,
    /// `config.json` — architecture parameters.
    Config,
    /// `generation_config.json` — default decoding parameters for LLMs.
    GenerationConfig,
}

/// One file to fetch for a model, relative to its repo root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFile {
    /// Path within the Hugging Face repo, e.g. `onnx/model_q4f16.onnx`.
    pub repo_path: String,
    /// What this file is.
    pub role: FileRole,
    /// Expected sha256 (lowercase hex). `None` means "not yet pinned": the file still downloads
    /// but [`verify`](super::verify) records it as unverified rather than failing — the hashes
    /// are filled from the repo's LFS metadata when a manifest is finalized.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Expected size in bytes, when known (a cheap pre-hash sanity check and a UI progress total).
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

impl ModelFile {
    fn new(repo_path: &str, role: FileRole) -> ModelFile {
        ModelFile {
            repo_path: repo_path.to_string(),
            role,
            sha256: None,
            size_bytes: None,
        }
    }

    /// The bare filename (last path segment), which is what the cache stores on disk.
    pub fn file_name(&self) -> &str {
        self.repo_path.rsplit('/').next().unwrap_or(&self.repo_path)
    }
}

/// The complete, self-contained description of one downloadable model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelManifest {
    /// Stable local id used in config (`embedding.model_id`, `llm.local_model`) and on disk.
    pub id: String,
    /// Human-readable name for the management UI.
    pub display_name: String,
    /// Hugging Face repo, e.g. `onnx-community/gemma-4-E2B-it-ONNX`.
    pub repo: String,
    /// Repo revision to pin downloads to (a branch, tag, or commit). `main` for the POC.
    pub revision: String,
    pub kind: ModelKind,
    pub quantization: Quantization,
    /// Every file to fetch (weights + tokenizer + configs).
    pub files: Vec<ModelFile>,
    /// Native hidden size: the embedding dimension for an embedder, the model width for an LLM.
    pub hidden_size: usize,
    /// Maximum context length in tokens the export supports.
    pub max_context_tokens: usize,
    /// Approximate RAM needed to load and run; gates the optional larger models behind a check.
    pub min_ram_mb: u32,
    /// Execution providers this model runs well on, best first; intersected with what the host
    /// offers in [`select_execution_provider`](super::execution_provider::select_execution_provider).
    pub preferred_execution_providers: Vec<ExecutionProvider>,
    /// How to pool token states into one vector. `Some` for embedders, `None` for LLMs.
    #[serde(default)]
    pub pooling: Option<PoolingStrategy>,
    /// Instruction template wrapped around a *query* for an asymmetric embedder (Qwen3 embeds
    /// queries and documents differently). `{query}` is substituted. `None` for symmetric models.
    #[serde(default)]
    pub query_instruction: Option<String>,
}

impl ModelManifest {
    /// The single weights graph file (`FileRole::Weights`).
    pub fn weights_file(&self) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.role == FileRole::Weights)
    }

    /// Token embedding graph for split decoder LLM exports.
    pub fn token_embeddings_file(&self) -> Option<&ModelFile> {
        self.files
            .iter()
            .find(|f| f.role == FileRole::TokenEmbeddings)
    }

    /// Decoder graph for split decoder LLM exports.
    pub fn decoder_file(&self) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.role == FileRole::Decoder)
    }

    /// The graph file to run for text generation.
    pub fn llm_decoder_graph_file(&self) -> Option<&ModelFile> {
        self.decoder_file().or_else(|| self.weights_file())
    }

    /// The tokenizer file (`FileRole::Tokenizer`).
    pub fn tokenizer_file(&self) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.role == FileRole::Tokenizer)
    }
}

/// All models the app ships knowledge of. Downloaded lazily on first use; nothing here implies
/// the bytes are present (see [`ModelCache`](super::cache::ModelCache)).
pub fn builtin_manifests() -> Vec<ModelManifest> {
    vec![qwen3_embedding_0_6b(), gemma_4_e2b()]
}

/// Look up a built-in manifest by its [`ModelManifest::id`].
pub fn builtin(id: &str) -> Option<ModelManifest> {
    builtin_manifests().into_iter().find(|m| m.id == id)
}

/// Stable id of the bundled local LLM (Phase 3).
pub const BUNDLED_LLM_ID: &str = "gemma-4-e2b";
/// Stable id of the default ONNX embedder (Phase 2).
pub const QWEN3_EMBEDDING_ID: &str = "qwen3-embedding-0.6b";

/// [Qwen3-Embedding-0.6B](https://huggingface.co/onnx-community/Qwen3-Embedding-0.6B-ONNX):
/// a 1024-dim causal embedder with last-token pooling, 32k context, and Matryoshka dims that can
/// be truncated for cheaper storage. Asymmetric — queries take an instruction prefix, documents
/// do not. Shares the ONNX runtime and tokenizer family with the bundled LLM below.
fn qwen3_embedding_0_6b() -> ModelManifest {
    ModelManifest {
        id: QWEN3_EMBEDDING_ID.to_string(),
        display_name: "Qwen3 Embedding 0.6B".to_string(),
        repo: "onnx-community/Qwen3-Embedding-0.6B-ONNX".to_string(),
        revision: "main".to_string(),
        kind: ModelKind::Embedding,
        quantization: Quantization::Int8,
        files: vec![
            ModelFile::new("onnx/model_quantized.onnx", FileRole::Weights),
            ModelFile::new("tokenizer.json", FileRole::Tokenizer),
            ModelFile::new("tokenizer_config.json", FileRole::TokenizerConfig),
            ModelFile::new("config.json", FileRole::Config),
        ],
        hidden_size: 1024,
        max_context_tokens: 32_768,
        min_ram_mb: 2_048,
        preferred_execution_providers: vec![
            ExecutionProvider::CoreMl,
            ExecutionProvider::Cuda,
            ExecutionProvider::DirectMl,
        ],
        pooling: Some(PoolingStrategy::LastToken),
        // The instruction Qwen3-Embedding recommends for retrieval queries (documents are raw).
        query_instruction: Some(
            "Instruct: Given a search query, retrieve relevant notes\nQuery: {query}".to_string(),
        ),
    }
}

/// [Gemma 4 E2B IT](https://huggingface.co/onnx-community/gemma-4-E2B-it-ONNX): the bundled
/// local LLM (Phase 3). The text path is a split ORT export: token ids first pass through
/// `embed_tokens_q4.onnx`, then embeddings feed `decoder_model_merged_q4.onnx`. Both graphs
/// have external `.onnx_data` files, so the manifest must download them next to the graphs.
///
/// The QAT-mobile repo is not a drop-in replacement for this manifest today: its text graphs are
/// named `q2f16`, while the built-in local model path requires a Q4 text export.
fn gemma_4_e2b() -> ModelManifest {
    ModelManifest {
        id: BUNDLED_LLM_ID.to_string(),
        display_name: "Gemma 4 E2B".to_string(),
        repo: "onnx-community/gemma-4-E2B-it-ONNX".to_string(),
        revision: "main".to_string(),
        kind: ModelKind::Llm,
        quantization: Quantization::Q4,
        files: vec![
            ModelFile::new("onnx/embed_tokens_q4.onnx", FileRole::TokenEmbeddings),
            ModelFile::new("onnx/embed_tokens_q4.onnx_data", FileRole::WeightsData),
            ModelFile::new("onnx/decoder_model_merged_q4.onnx", FileRole::Decoder),
            ModelFile::new(
                "onnx/decoder_model_merged_q4.onnx_data",
                FileRole::WeightsData,
            ),
            ModelFile::new("tokenizer.json", FileRole::Tokenizer),
            ModelFile::new("tokenizer_config.json", FileRole::TokenizerConfig),
            ModelFile::new("config.json", FileRole::Config),
            ModelFile::new("generation_config.json", FileRole::GenerationConfig),
        ],
        hidden_size: 2048,
        max_context_tokens: 8_192,
        min_ram_mb: 3_072,
        preferred_execution_providers: vec![
            ExecutionProvider::CoreMl,
            ExecutionProvider::Cuda,
            ExecutionProvider::DirectMl,
        ],
        pooling: None,
        query_instruction: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_one_embedder_and_one_llm() {
        let all = builtin_manifests();
        assert_eq!(
            all.iter()
                .filter(|m| m.kind == ModelKind::Embedding)
                .count(),
            1
        );
        assert_eq!(all.iter().filter(|m| m.kind == ModelKind::Llm).count(), 1);
    }

    #[test]
    fn lookup_by_id_round_trips() {
        let m = builtin(BUNDLED_LLM_ID).expect("bundled llm present");
        assert_eq!(m.kind, ModelKind::Llm);
        assert_eq!(m.quantization, Quantization::Q4);
        assert!(builtin("no-such-model").is_none());
    }

    #[test]
    fn embedder_is_asymmetric_with_last_token_pooling() {
        let m = builtin(QWEN3_EMBEDDING_ID).unwrap();
        assert_eq!(m.pooling, Some(PoolingStrategy::LastToken));
        assert!(m.query_instruction.as_ref().unwrap().contains("{query}"));
        assert_eq!(m.hidden_size, 1024);
    }

    #[test]
    fn every_model_has_runnable_graph_and_a_tokenizer() {
        for m in builtin_manifests() {
            let has_graph = match m.kind {
                ModelKind::Embedding => m.weights_file().is_some(),
                ModelKind::Llm => m.llm_decoder_graph_file().is_some(),
            };
            assert!(has_graph, "{} lacks runnable graph", m.id);
            assert!(m.tokenizer_file().is_some(), "{} lacks a tokenizer", m.id);
        }
    }

    #[test]
    fn gemma_manifest_matches_split_text_export() {
        let m = builtin(BUNDLED_LLM_ID).unwrap();
        assert_eq!(m.repo, "onnx-community/gemma-4-E2B-it-ONNX");
        assert_ne!(m.repo, "onnx-community/gemma-4-E2B-it-qat-mobile-ONNX");
        assert_eq!(m.quantization, Quantization::Q4);
        assert_eq!(
            m.token_embeddings_file().unwrap().repo_path,
            "onnx/embed_tokens_q4.onnx"
        );
        assert_eq!(
            m.decoder_file().unwrap().repo_path,
            "onnx/decoder_model_merged_q4.onnx"
        );
        assert_eq!(
            m.files
                .iter()
                .filter(|f| f.role == FileRole::WeightsData)
                .count(),
            2
        );
    }

    #[test]
    fn file_name_strips_repo_subdir() {
        let f = ModelFile::new("onnx/model_q4f16.onnx", FileRole::Weights);
        assert_eq!(f.file_name(), "model_q4f16.onnx");
    }

    #[test]
    fn manifest_serializes_to_yaml_round_trip() {
        // Manifests live in config, so they must survive a serde round trip cleanly.
        let m = gemma_4_e2b();
        let yaml = serde_yaml::to_string(&m).unwrap();
        let back: ModelManifest = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(m, back);
    }
}
