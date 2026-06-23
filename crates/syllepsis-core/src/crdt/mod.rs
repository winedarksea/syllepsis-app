//! Per-note CRDT documents: the convergent-merge layer Phase 4 sync is built on.
//!
//! The note's markdown body is the source of truth for what the user *sees*; a CRDT **sidecar**
//! (`_crdt/{ulid}.crdt`) records the convergent edit history so that when the same note is edited
//! on two devices, the two versions *merge* on the next sync instead of one clobbering the other.
//!
//! The seam is two traits — exactly the pattern [`embeddings`](crate::embeddings) and
//! [`llm`](crate::llm) use:
//! - [`NoteCrdt`] — the per-document operations (text, local edit, snapshot, merge). Object-safe
//!   so the sync engine holds a `Box<dyn NoteCrdt>` and never names a backend.
//! - [`CrdtBackend`] — the factory that constructs/loads documents for a chosen backend.
//!
//! Two backends implement them. [`LwwTextDocument`](lww::LwwTextDocument) is the always-compiled
//! default: a deterministic last-writer-wins register over the whole body, keyed by a hybrid
//! logical clock. It is a genuine CRDT (a total-order max — commutative, associative, idempotent)
//! and converges correctly for the primary design target (one user, several devices, rarely truly
//! concurrent). [`LoroDocument`](loro_backend::LoroDocument), behind the optional `loro` feature,
//! is the fine-grained text CRDT that merges concurrent edits to *different* parts of a note.

#[cfg(feature = "loro")]
mod loro_backend;
mod lww;

pub use lww::{LwwBackend, LwwTextDocument};

use crate::config::SyncConfig;
use crate::error::CoreResult;

/// Config value of [`SyncConfig::crdt_backend`] selecting the built-in LWW register backend.
pub const LWW_BACKEND: &str = "lww";
/// Config value selecting the fine-grained Loro text CRDT (requires the `loro` feature).
pub const LORO_BACKEND: &str = "loro";

/// A stable per-device identity. CRDTs need to attribute edits to a replica so concurrent changes
/// are distinguishable and tie-breaks are deterministic. Persisted once per machine under `_sync/`
/// (device-local, never synced — every device must have its *own* actor).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActorId(String);

impl ActorId {
    pub fn new(id: impl Into<String>) -> ActorId {
        ActorId(id.into())
    }

    /// Mint a fresh, globally-unique actor id (a ulid — already a crate dependency).
    pub fn generate() -> ActorId {
        ActorId(ulid::Ulid::new().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One note's convergent document. Implementations must keep [`merge`](NoteCrdt::merge)
/// commutative, associative, and idempotent so sync order never changes the result.
pub trait NoteCrdt: Send {
    /// Backend identifier recorded for diagnostics (`lww`, `loro`).
    fn backend(&self) -> &'static str;

    /// The current merged text (what the markdown body is rendered from).
    fn text(&self) -> String;

    /// Apply a local edit setting the document text to `new_text`. Setting it to the value it
    /// already holds must be a no-op (no new operation) — this is what makes re-running sync after
    /// no real change produce identical bytes and avoid write loops.
    fn set_text(&mut self, new_text: &str);

    /// Serialize the current state to a sidecar snapshot.
    fn snapshot(&self) -> CoreResult<Vec<u8>>;

    /// Merge another replica's snapshot into this document.
    fn merge(&mut self, snapshot: &[u8]) -> CoreResult<()>;

    /// Serialize the document's current version vector as JSON. Backends that cannot produce
    /// incremental updates return a clear sync error.
    fn version_vector_json(&self) -> CoreResult<String> {
        Err(crate::error::CoreError::Sync(format!(
            "{} does not support incremental sync",
            self.backend()
        )))
    }

    /// Export updates since the provided JSON version vector.
    fn updates_since_json(&self, _version_vector_json: &str) -> CoreResult<Vec<u8>> {
        Err(crate::error::CoreError::Sync(format!(
            "{} does not support incremental sync",
            self.backend()
        )))
    }

    /// Import an incremental update payload.
    fn import_updates(&mut self, _updates: &[u8]) -> CoreResult<()> {
        Err(crate::error::CoreError::Sync(format!(
            "{} does not support incremental sync",
            self.backend()
        )))
    }
}

/// Constructs [`NoteCrdt`] documents for one backend. The sync engine selects one of these once
/// and treats every note's sidecar through it.
pub trait CrdtBackend: Send + Sync {
    fn name(&self) -> &'static str;

    /// A fresh document for `actor`, seeded with `text` (a brand-new note, or first sidecar for a
    /// note that until now was markdown-only).
    fn new_document(&self, actor: &ActorId, text: &str) -> Box<dyn NoteCrdt>;

    /// Rehydrate a document from a persisted sidecar snapshot, attributing future local edits to
    /// `actor`.
    fn load_document(&self, actor: &ActorId, snapshot: &[u8]) -> CoreResult<Box<dyn NoteCrdt>>;
}

/// Pick the CRDT backend for a book. Mirrors [`llm::select_llm_provider`](crate::llm) and the
/// embedder selection: the fine-grained Loro backend only when it was both requested *and* compiled
/// in; otherwise the always-available LWW register, so sync works in every build.
pub fn select_crdt_backend(cfg: &SyncConfig) -> Box<dyn CrdtBackend> {
    #[cfg(feature = "loro")]
    if cfg.crdt_backend == LORO_BACKEND {
        return Box::new(loro_backend::LoroBackend);
    }
    #[cfg(not(feature = "loro"))]
    let _ = cfg;
    Box::new(LwwBackend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_lww_backend_round_trips() {
        let cfg = SyncConfig {
            crdt_backend: LWW_BACKEND.to_string(),
            ..SyncConfig::default()
        };
        let backend = select_crdt_backend(&cfg);
        assert_eq!(backend.name(), LWW_BACKEND);
        let actor = ActorId::new("device-a");
        let doc = backend.new_document(&actor, "hello");
        let snap = doc.snapshot().unwrap();
        let reloaded = backend.load_document(&actor, &snap).unwrap();
        assert_eq!(reloaded.text(), "hello");
    }

    #[test]
    #[cfg(feature = "loro")]
    fn default_backend_is_loro_when_feature_is_enabled() {
        let backend = select_crdt_backend(&SyncConfig::default());
        assert_eq!(backend.name(), LORO_BACKEND);
        let actor = ActorId::new("device-a");
        let doc = backend.new_document(&actor, "hello");
        let snap = doc.snapshot().unwrap();
        let reloaded = backend.load_document(&actor, &snap).unwrap();
        assert_eq!(reloaded.text(), "hello");
    }

    #[test]
    #[cfg(not(feature = "loro"))]
    fn default_backend_falls_back_to_lww_without_loro_feature() {
        assert_eq!(
            select_crdt_backend(&SyncConfig::default()).name(),
            LWW_BACKEND
        );
    }

    #[test]
    fn unknown_backend_falls_back_to_lww() {
        // Requesting loro without the feature (or a typo'd backend) must not break sync.
        let cfg = SyncConfig {
            crdt_backend: "no-such-backend".to_string(),
            ..SyncConfig::default()
        };
        assert_eq!(select_crdt_backend(&cfg).name(), LWW_BACKEND);
    }
}
