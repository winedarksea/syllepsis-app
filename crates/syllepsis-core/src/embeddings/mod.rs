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
pub mod note;
pub mod pooling;
pub mod provider;
pub mod selection;
pub mod vector;

#[cfg(feature = "onnx")]
pub mod onnx;

pub use chunk::{chunk_text, Chunk};
pub use hashing::HashingEmbedder;
pub use note::{category_vector, embed_note, embed_notes, NoteVectors};
pub use provider::{EmbeddingProvider, ProviderInfo};
pub use selection::select_embedder;
pub use vector::Embedding;

#[cfg(feature = "onnx")]
pub use onnx::OnnxEmbedder;
