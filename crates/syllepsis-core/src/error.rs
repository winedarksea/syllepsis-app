//! Crate-wide error type. Every fallible public API returns [`CoreResult`].
//!
//! We keep one flat enum rather than per-module errors: the app surface is small,
//! and a single type keeps the Tauri command boundary trivial to serialize.

use thiserror::Error;

/// Convenience alias used throughout the crate.
pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    /// A note/category/world id did not resolve against the store.
    #[error("not found: {0}")]
    NotFound(String),

    /// A supplied id string was structurally invalid (bad `{type}-{slug}-{ulid}`).
    #[error("invalid id: {0}")]
    InvalidId(String),

    /// Frontmatter or body could not be parsed into the expected shape.
    #[error("parse error in {context}: {message}")]
    Parse { context: String, message: String },

    /// A prior/sort edge would violate an invariant (e.g. a cycle, or a note as a
    /// category's parent).
    #[error("invalid sort relationship: {0}")]
    InvalidSort(String),

    /// A book directory operation would target an invalid or unsafe location.
    #[error("invalid book: {0}")]
    InvalidBook(String),

    /// Underlying filesystem failure from the storage layer.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML (frontmatter) (de)serialization failure.
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON (registry / sidecar) (de)serialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// An LLM provider failed, was not configured, or returned an unusable response.
    #[error("llm error: {0}")]
    Llm(String),

    /// A locked note's protected body cannot change yet: the unlock delay has not elapsed or the
    /// required fact-check has not passed (privacy-security.md "Locked Files").
    #[error("locked: {0}")]
    Locked(String),

    /// A local model could not be fetched, failed integrity verification, or the ONNX runtime
    /// could not load/run it (the shared embedding+LLM model-runtime pattern).
    #[error("model error: {0}")]
    Model(String),

    /// A sync or CRDT operation failed: a cloud-storage provider I/O error, a CRDT
    /// merge/snapshot (de)serialization failure, or sync-state bookkeeping (Phase 4).
    #[error("sync error: {0}")]
    Sync(String),

    /// A plugin could not be loaded, was misconfigured, or failed while executing: a malformed
    /// manifest, a missing/invalid WASM module, or an error returned from a plugin call.
    #[error("plugin error: {0}")]
    Plugin(String),
}

impl CoreError {
    /// Helper for constructing a [`CoreError::Parse`] without repeating the struct
    /// boilerplate at call sites.
    pub fn parse(context: impl Into<String>, message: impl Into<String>) -> Self {
        CoreError::Parse {
            context: context.into(),
            message: message.into(),
        }
    }
}
