//! Local, device-side embeddings: the vector half of search and the engine behind the
//! related-carousel and embedding diagnostics.
//!
//! Everything here is platform-agnostic and behind the [`EmbeddingProvider`] seam. The default
//! provider ([`HashingEmbedder`]) needs no model and no network, so the whole pipeline —
//! chunking, per-note/per-category vectors, similarity — works and is fully tested today; an
//! ONNX model run via `ort` / Transformers.js (see llm-ai-features.md, "Local Embeddings")
//! slots in as another `impl EmbeddingProvider` without touching callers. Persisting vectors to
//! persistent vector storage in `_derived/` sits below this same API.

pub mod chunk;
pub mod hashing;
pub mod input;
pub mod note;
pub mod pooling;
pub mod provider;
pub mod repository;
pub mod selection;
pub mod sidecar;
pub mod vector;

#[cfg(feature = "onnx")]
pub mod onnx;

pub use chunk::{chunk_text, Chunk};
pub use hashing::HashingEmbedder;
pub use note::{category_vector, embed_note, embed_notes, NoteVectors};
pub use provider::{EmbeddingProvider, ProviderInfo};
pub use repository::{
    configured_model_fingerprint, generate_note_sidecar, load_embedding_corpus,
    note_embedding_is_stale, sidecar_preference_rank, stale_or_missing_note_ids,
    EmbeddingCoverage, LoadedEmbeddingCorpus,
};
pub use selection::{select_embedder, try_select_embedder};
pub use sidecar::{
    full_note_source_hash, read_sidecar, summary_source_hash, write_sidecar_atomic,
    EmbeddingModelFingerprint, NoteEmbeddingSidecar, StoredEmbedding, INPUT_POLICY_VERSION,
};
pub use vector::Embedding;

#[cfg(feature = "onnx")]
pub use onnx::OnnxEmbedder;
