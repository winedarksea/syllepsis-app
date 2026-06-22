//! # syllepsis-core
//!
//! Platform-agnostic domain logic for Syllepsis. Contains no Tauri, no UI, and no
//! platform-specific I/O beyond the [`storage::NoteStore`] trait — the native filesystem
//! implementation is one impl behind that seam, leaving room for an OPFS impl in the PWA
//! build.
//!
//! Module layout mirrors the design docs:
//! - [`id`] — the `{type}-{slug}-{ulid}` identity scheme.
//! - [`model`] — note/category/world object types and the full metadata schema.
//! - [`markdown`] — frontmatter + the Syllepsis markdown dialect (`%%`, `||cloze||`, `loc:`…).
//! - [`storage`] — book folder layout, the note store, and the id registry.
//! - [`sort`] — building the prior-relationship tree and flattening it into book view.
//! - [`spatial`] — worlds & overlays: the `loc:` grammar, the text→coordinate lookup, and pins/
//!   regions over an image-backed or geo world (floorplans, mind palaces, geo-tags).
//! - [`embeddings`] — local vectors behind the [`EmbeddingProvider`] seam (chunking, similarity).
//! - [`search`] — exact + BM25 + vector retrieval fused with RRF, plus category facets.
//! - [`llm`] — optional, per-task LLM features behind the [`llm::LlmProvider`] seam.
//! - [`onnx`] — the shared model-runtime pattern (manifest, download, cache, verify, execution
//!   providers) the local embedder and local LLM both build on.
//! - [`text`] — the single tokenizer shared by embeddings and search.

pub mod app;
pub mod config;
pub mod crdt;
pub mod embeddings;
pub mod error;
pub mod id;
pub mod llm;
pub mod markdown;
pub mod model;
pub mod onnx;
pub mod search;
pub mod sort;
pub mod spatial;
pub mod storage;
pub mod sync;
pub mod text;

pub use config::{Config, ModelRef};
pub use crdt::{ActorId, CrdtBackend, NoteCrdt};
pub use embeddings::{Embedding, EmbeddingProvider, HashingEmbedder};
pub use error::{CoreError, CoreResult};
pub use id::NoteId;
pub use llm::{LlmProvider, LlmService, LlmTask, Proposal, ProposalStatus};
pub use model::{Category, Metadata, Note, ObjectType, PriorEdge, PriorKind};
pub use onnx::{ModelCache, ModelKind, ModelManifest, RuntimeDiagnostics};
pub use search::{SearchEngine, SearchHit, SearchResults};
pub use model::{SpatialRegion, World, WorldKind};
pub use sort::{render, to_markdown, RenderItem};
pub use spatial::{
    build_overlay, LocationLookup, Overlay, ResolvedLocation, WorldPoint, WorldRegistry,
};
pub use storage::{Book, BookMetadata, NoteStore};
pub use sync::{LocalFolderSync, SyncEngine, SyncProvider, SyncReport};
