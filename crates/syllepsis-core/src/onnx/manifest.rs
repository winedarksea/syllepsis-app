//! The model manifest: the single config-driven description of a downloadable ONNX model that
//! the whole runtime pattern keys off.
//!
//! One manifest names everything needed to fetch and run a model — its Hugging Face repo, the
//! exact files (graphs, external data, tokenizer, configs) with optional sha256s, its quantization, native
//! width, context length, RAM floor, preferred execution providers, and (for embedders) how to
//! pool and prompt. Adding a model — the optional larger LLM, a different embedder — is a new
//! manifest entry, never a code change, which is what lets the LLM and the embedder share this
//! exact machinery (llm-ai-features.md, "the same config-driven model manifest"). The built-in
//! registry below ships the embedding and generation models the app bundles.

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
    fn pinned(repo_path: &str, role: FileRole, sha256: &str, size_bytes: u64) -> ModelFile {
        ModelFile {
            repo_path: repo_path.to_string(),
            role,
            sha256: Some(sha256.to_string()),
            size_bytes: Some(size_bytes),
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
    /// Compatible accelerated execution providers, best first; intersected with what the host
    /// offers in [`select_execution_provider`](super::execution_provider::select_execution_provider).
    /// Empty delegates to platform defaults. CPU remains the universal fallback.
    pub preferred_execution_providers: Vec<ExecutionProvider>,
    /// How to pool token states into one vector. `Some` for embedders, `None` for LLMs.
    #[serde(default)]
    pub pooling: Option<PoolingStrategy>,
    /// Instruction template wrapped around a *query* for an asymmetric embedder (queries and
    /// queries and documents differently). `{query}` is substituted. `None` for symmetric models.
    #[serde(default)]
    pub query_instruction: Option<String>,
    /// Document-side template. `{title}` and `{text}` are substituted before tokenization.
    #[serde(default)]
    pub document_instruction: Option<String>,
    /// Preferred named embedding output. Some exports return the pooled sentence vector directly.
    #[serde(default)]
    pub embedding_output_name: Option<String>,
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
    vec![embeddinggemma_300m(), gemma_4_e2b()]
}

/// Look up a built-in manifest by its [`ModelManifest::id`].
pub fn builtin(id: &str) -> Option<ModelManifest> {
    let canonical_id = match id {
        LEGACY_QWEN3_EMBEDDING_ID => EMBEDDINGGEMMA_ID,
        _ => id,
    };
    builtin_manifests()
        .into_iter()
        .find(|manifest| manifest.id == canonical_id)
}

/// Stable id of the bundled local LLM (Phase 3).
pub const BUNDLED_LLM_ID: &str = "gemma-4-e2b";
/// Stable id of the default ONNX embedder (Phase 2).
pub const EMBEDDINGGEMMA_ID: &str = "embeddinggemma-300m";
/// Previous default retained only as a read-time alias so an old book never loses search/graph
/// functionality before its config migration is persisted.
pub const LEGACY_QWEN3_EMBEDDING_ID: &str = "qwen3-embedding-0.6b";

/// EmbeddingGemma 300M Q4: 768 native dimensions, 2k context, direct sentence-vector output, and
/// Matryoshka truncation to the configured 256 dimensions.
fn embeddinggemma_300m() -> ModelManifest {
    ModelManifest {
        id: EMBEDDINGGEMMA_ID.to_string(),
        display_name: "EmbeddingGemma 300M".to_string(),
        repo: "onnx-community/embeddinggemma-300m-ONNX".to_string(),
        revision: "5090578d9565bb06545b4552f76e6bc2c93e4a66".to_string(),
        kind: ModelKind::Embedding,
        quantization: Quantization::Q4,
        files: vec![
            ModelFile::pinned(
                "onnx/model_q4.onnx",
                FileRole::Weights,
                "ad1dfee81a70f7944b9b9d1cc6e48075b832881cf33fab2f2b248be78f3f0043",
                519_322,
            ),
            ModelFile::pinned(
                "onnx/model_q4.onnx_data",
                FileRole::WeightsData,
                "599962c3143b040de2dd05e5975be3e9091dd067cacc6a8f7186e3203bab9e02",
                196_725_760,
            ),
            ModelFile::pinned(
                "tokenizer.json",
                FileRole::Tokenizer,
                "4dda02faaf32bc91031dc8c88457ac272b00c1016cc679757d1c441b248b9c47",
                20_323_312,
            ),
        ],
        hidden_size: 768,
        max_context_tokens: 2_048,
        min_ram_mb: 512,
        preferred_execution_providers: vec![ExecutionProvider::Cuda, ExecutionProvider::DirectMl],
        pooling: Some(PoolingStrategy::Mean),
        query_instruction: Some("task: search result | query: {query}".to_string()),
        document_instruction: Some("title: {title} | text: {text}".to_string()),
        embedding_output_name: Some("sentence_embedding".to_string()),
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
            ModelFile::pinned(
                "onnx/embed_tokens_q4.onnx",
                FileRole::TokenEmbeddings,
                "2d8c8a2bcc30e8ded7f636967c2a58a346116583356dd933720b005fc88079c4",
                5_142,
            ),
            ModelFile::pinned(
                "onnx/embed_tokens_q4.onnx_data",
                FileRole::WeightsData,
                "40fa957d9988b8a0160c8b0eb5c3f781a237627e9f7153f30514a4ffb2e62888",
                1_762_656_256,
            ),
            ModelFile::pinned(
                "onnx/decoder_model_merged_q4.onnx",
                FileRole::Decoder,
                "c6edb929bf342c524728d37efd400285ee71525e8fe64ff996341f78c3e577d2",
                647_599,
            ),
            ModelFile::pinned(
                "onnx/decoder_model_merged_q4.onnx_data",
                FileRole::WeightsData,
                "b879fe4b946c9b9ff6acb60f7c5eda3d2c9c4df8625895feb2d1e269002f0345",
                1_864_102_912,
            ),
            ModelFile::pinned(
                "tokenizer.json",
                FileRole::Tokenizer,
                "47bd35616c7c782aaca6ccf48c75f3461d5877170984b8836b375107d0a9f566",
                19_439_251,
            ),
            ModelFile::pinned(
                "tokenizer_config.json",
                FileRole::TokenizerConfig,
                "06afbf54e228050cba79c4a0afd83543cc89070a2d62b8337d0aa8b4cdc348c3",
                18_807,
            ),
            ModelFile::pinned(
                "config.json",
                FileRole::Config,
                "5494e6677d9e150ea20ba3101ae8a32b0f141004626f052725d8bf48991b9faa",
                5_549,
            ),
            ModelFile::pinned(
                "generation_config.json",
                FileRole::GenerationConfig,
                "e6a0b50de21a511f15ac4857b7f227f68ee60ecb1f11255d07b75e0bdc60e155",
                238,
            ),
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
        document_instruction: None,
        embedding_output_name: None,
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
        assert_eq!(
            builtin(LEGACY_QWEN3_EMBEDDING_ID).unwrap().id,
            EMBEDDINGGEMMA_ID
        );
        assert!(builtin("no-such-model").is_none());
    }

    #[test]
    fn embeddinggemma_has_document_and_query_prompts() {
        let m = builtin(EMBEDDINGGEMMA_ID).unwrap();
        assert_eq!(m.pooling, Some(PoolingStrategy::Mean));
        assert!(m.query_instruction.as_ref().unwrap().contains("{query}"));
        assert!(m.document_instruction.as_ref().unwrap().contains("{text}"));
        assert_eq!(m.hidden_size, 768);
        assert_eq!(m.max_context_tokens, 2048);
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
    fn every_builtin_model_file_has_pinned_integrity_metadata() {
        for manifest in builtin_manifests() {
            for file in &manifest.files {
                assert!(
                    file.sha256
                        .as_ref()
                        .is_some_and(|hash| hash.len() == 64
                            && hash.chars().all(|ch| ch.is_ascii_hexdigit())),
                    "{}:{} lacks a pinned sha256",
                    manifest.id,
                    file.repo_path
                );
                assert!(
                    file.size_bytes.is_some_and(|size| size > 0),
                    "{}:{} lacks a positive size",
                    manifest.id,
                    file.repo_path
                );
            }
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
        let f = ModelFile::pinned(
            "onnx/model_q4f16.onnx",
            FileRole::Weights,
            &"0".repeat(64),
            1,
        );
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
