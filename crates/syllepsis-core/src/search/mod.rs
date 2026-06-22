//! Retrieval: exact + BM25 + vector search fused with Reciprocal Rank Fusion, plus the
//! category facets, related-notes, and embedding diagnostics built on the same index.
//!
//! [`SearchEngine`] is the single entry point; the submodules are its independently testable
//! pieces ([`bm25`], [`exact`], [`rrf`]) and the embedding vectors come from
//! [`crate::embeddings`]. [`sqlite_index`] persists the same snapshot into `_derived/search.sqlite`
//! with FTS5 plus stored vectors for local-first indexing.

pub mod bm25;
pub mod engine;
pub mod exact;
pub mod results;
pub mod rrf;
pub mod sqlite_index;

pub use engine::SearchEngine;
pub use results::{
    BlindSpot, DuplicatePair, EmbeddingDiagnostics, FacetCount, RelatedNote, SearchHit,
    SearchResults,
};
pub use sqlite_index::SqliteSearchIndex;
