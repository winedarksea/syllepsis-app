//! Operational configuration. Per AGENTS.md there are **no magic numbers** scattered
//! through the code: every threshold, timing, and default lives here as a typed field
//! so it can be persisted per-book and tuned without code changes.
//!
//! Defaults encode the values called out in the design docs (summary warning limits,
//! vanish/deletion delays, unlock delay, chunk size, RRF constant).

use serde::{Deserialize, Serialize};

/// Root configuration grouped into domain-specific sub-configs (one reason to change each).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub markdown: MarkdownConfig,
    pub summary: SummaryConfig,
    pub cleanup: CleanupConfig,
    pub privacy: PrivacyConfig,
    pub embedding: EmbeddingConfig,
    pub search: SearchConfig,
    pub llm: LlmConfig,
    pub sync: SyncConfig,
}

/// The markdown dialect identifier written to every file's frontmatter so files read
/// outside the app can be traced back to their origin format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    pub dialect_version: String,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        MarkdownConfig {
            dialect_version: "syllepsis_001".to_string(),
        }
    }
}

/// Thresholds for the summary/full-description alignment warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryConfig {
    /// Warn if the summary exceeds this many characters...
    pub max_chars: usize,
    /// ...or exceeds this fraction of the full description (whichever limit is larger).
    pub max_fraction_of_body: f32,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        SummaryConfig {
            max_chars: 250,
            max_fraction_of_body: 0.10,
        }
    }
}

/// Timings for archival, vanishing notes, and delayed deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    /// Default lifespan for a "vanishing" note set at creation.
    pub default_vanish_days: u32,
    /// Delay between "mark for deletion" and permanent removal.
    pub deletion_delay_days: u32,
    /// Days a done/cancelled todo line lingers before moving to the todo archive.
    pub todo_archive_days: u32,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        CleanupConfig {
            default_vanish_days: 180,
            deletion_delay_days: 30,
            todo_archive_days: 14,
        }
    }
}

/// Self-protection delays for locked/deleted content (protecting the user from themselves).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Hours before a proposed rewrite to a locked note may be merged.
    pub unlock_delay_hours: u32,
    /// Hours before a delete/unlock confirmation takes effect.
    pub confirmation_delay_hours: u32,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        PrivacyConfig {
            unlock_delay_hours: 24,
            confirmation_delay_hours: 24,
        }
    }
}

/// Embedding/chunking parameters for the local vector pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// Notes longer than this many tokens are chunked, one vector per chunk.
    pub chunk_token_limit: usize,
    /// Token overlap between adjacent chunks to preserve context at boundaries.
    pub chunk_overlap_tokens: usize,
    /// Vector dimensionality. The default [`crate::embeddings`] provider is a deterministic
    /// feature-hashing embedder at this width; an ONNX model swapped in behind the
    /// `EmbeddingProvider` seam fixes its own native dimension and ignores this (Qwen3-Embedding
    /// is 1024).
    pub dimensions: usize,
    /// Which embedding model to use. The default names the built-in Qwen3 ONNX embedder; until it
    /// is downloaded, the provider selector falls back to deterministic feature hashing.
    pub model_id: String,
    /// Optional Matryoshka truncation for the ONNX embedder: keep only the first N dimensions of
    /// each vector (re-normalized) for cheaper storage. `None` keeps the model's native width.
    /// Ignored by the hashing embedder.
    pub matryoshka_dims: Option<usize>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        EmbeddingConfig {
            chunk_token_limit: 512,
            chunk_overlap_tokens: 64,
            dimensions: 256,
            model_id: crate::onnx::manifest::QWEN3_EMBEDDING_ID.to_string(),
            matryoshka_dims: None,
        }
    }
}

/// Search fusion and ranking parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Reciprocal Rank Fusion constant `k` (smoothing factor over result ranks).
    pub rrf_k: f32,
    /// How much category membership upweights vector similarity in the related carousel.
    pub category_upweight: f32,
    /// BM25 term-frequency saturation parameter `k1`.
    pub bm25_k1: f32,
    /// BM25 length-normalization parameter `b` (0 = none, 1 = full).
    pub bm25_b: f32,
    /// Maximum hits returned by a full search query.
    pub result_limit: usize,
    /// Number of neighbors shown in the related carousel.
    pub related_limit: usize,
    /// Cosine-similarity floor above which two notes are flagged as near-duplicates by the
    /// embedding diagnostics.
    pub duplicate_similarity: f32,
    /// Cosine-similarity ceiling below which a note's nearest neighbor is so weak the note is
    /// flagged as a blind spot (poorly connected to the rest of the book).
    pub blind_spot_similarity: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            rrf_k: 60.0,
            category_upweight: 1.25,
            bm25_k1: 1.2,
            bm25_b: 0.75,
            result_limit: 50,
            related_limit: 8,
            duplicate_similarity: 0.92,
            blind_spot_similarity: 0.15,
        }
    }
}

/// LLM features. The default is no-config local inference: use the bundled ONNX model when it is
/// cached and the desktop build includes ONNX, otherwise fall back to the deterministic offline
/// provider so the proposal flow still works.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Master switch for cloud/local LLM calls. When false, the offline provider is used.
    pub enabled: bool,
    /// Identifier of the configured provider: `offline` (built-in heuristic), `local` (the
    /// bundled ONNX model, feature `onnx`), or a cloud provider (e.g. `anthropic`).
    pub provider: String,
    /// Manifest id of the bundled local model used when `provider = "local"`.
    pub local_model: String,
    /// Maximum tokens the local model generates per call (bounds latency on CPU).
    pub max_new_tokens: usize,
    /// Accept generated proposals automatically instead of queuing them for review.
    pub auto_accept: bool,
    /// Per-task model routing.
    pub routing: LlmRouting,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            enabled: true,
            provider: crate::llm::selection::LOCAL_PROVIDER.to_string(),
            local_model: crate::onnx::manifest::BUNDLED_LLM_ID.to_string(),
            max_new_tokens: 512,
            auto_accept: false,
            routing: LlmRouting::default(),
        }
    }
}

/// Sync & backup settings (Phase 4, sync-backup.md). Markdown stays the source of truth; a
/// per-note CRDT sidecar captures the convergent edit history so concurrent edits on different
/// devices merge instead of clobbering. Sync targets a user-owned cloud store behind the
/// [`SyncProvider`](crate::sync::SyncProvider) seam — the app hosts nothing itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Master switch. When false the sync engine never runs (pure local-file editing); CRDT
    /// sidecars are not maintained.
    pub enabled: bool,
    /// Which CRDT backend manages per-note sidecars: `lww` (built-in last-writer-wins register,
    /// always available, deterministic) or `loro` (fine-grained text CRDT — true character-level
    /// merge — which requires the `loro` Cargo feature; absent it, selection falls back to `lww`).
    pub crdt_backend: String,
    /// Filename marker for conflict copies the engine writes when two devices changed the same
    /// non-mergeable file (`{name}.conflict-{actor}.{ext}`). Conflict copies are themselves synced
    /// so every device sees the same set, then the user resolves and deletes them.
    pub conflict_marker: String,
    /// Clock-skew guard: a markdown file whose on-disk mtime leads its sidecar by less than this
    /// many seconds is treated as the same logical edit and not re-ingested as an external change.
    pub external_edit_skew_secs: i64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            enabled: true,
            crdt_backend: crate::crdt::LWW_BACKEND.to_string(),
            conflict_marker: "conflict".to_string(),
            external_edit_skew_secs: 2,
        }
    }
}

/// A concrete model on a concrete provider. Routing uses this instead of a bare model string so
/// "summarize locally, fact-check in cloud" is representable without guessing from model names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> ModelRef {
        ModelRef {
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn local_builtin() -> ModelRef {
        ModelRef::new(
            crate::llm::selection::LOCAL_PROVIDER,
            crate::onnx::manifest::BUNDLED_LLM_ID,
        )
    }
}

impl Default for ModelRef {
    fn default() -> Self {
        ModelRef::local_builtin()
    }
}

/// Which model handles each task (model-router pattern). Reasoning-heavy tasks (fact check,
/// devil's advocate, rewrites) route to the most capable model; mechanical tasks (summaries,
/// grammar, category suggestions) route to the fast, inexpensive one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmRouting {
    pub summarize: ModelRef,
    pub fact_check: ModelRef,
    pub devils_advocate: ModelRef,
    pub grammar: ModelRef,
    pub category_suggest: ModelRef,
    pub rewrite: ModelRef,
}

impl Default for LlmRouting {
    fn default() -> Self {
        let local = ModelRef::local_builtin();
        LlmRouting {
            summarize: local.clone(),
            fact_check: local.clone(),
            devils_advocate: local.clone(),
            grammar: local.clone(),
            category_suggest: local.clone(),
            rewrite: local,
        }
    }
}
