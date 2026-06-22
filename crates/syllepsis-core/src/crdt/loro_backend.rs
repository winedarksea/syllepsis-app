//! The fine-grained CRDT backend: [Loro](https://loro.dev), behind the optional `loro` feature.
//!
//! Where the default [`LwwTextDocument`](super::lww::LwwTextDocument) picks one whole-body winner,
//! Loro tracks the note body as a text CRDT and merges concurrent edits to *different* regions —
//! two devices appending different paragraphs both survive. Loro is the primary CRDT named in the
//! design (platform-infra.md); like the `ort` ML path it lives behind a feature so the default
//! build stays lean and the offline default always works.
//!
//! The note body is a single Loro text container (`body`). [`set_text`](NoteCrdt::set_text) uses
//! Loro's diffing `update`, so a local edit is recorded as the minimal insert/delete against the
//! current state — that is what lets two devices' edits coexist after [`merge`](NoteCrdt::merge),
//! which is a Loro snapshot `import`.

use std::hash::{Hash, Hasher};

use loro::{ExportMode, LoroDoc, UpdateOptions};

use crate::crdt::{ActorId, CrdtBackend, NoteCrdt};
use crate::error::{CoreError, CoreResult};

/// The text container holding the note body inside the Loro document.
const BODY_CONTAINER: &str = "body";

/// A note body as a Loro text CRDT.
pub struct LoroDocument {
    doc: LoroDoc,
}

impl LoroDocument {
    fn new(actor: &ActorId, text: &str) -> CoreResult<LoroDocument> {
        let doc = LoroDoc::new();
        set_actor(&doc, actor)?;
        let body = doc.get_text(BODY_CONTAINER);
        if !text.is_empty() {
            body.update(text, UpdateOptions::default())
                .map_err(|e| CoreError::Sync(format!("loro seed text: {e}")))?;
        }
        doc.commit();
        Ok(LoroDocument { doc })
    }

    fn from_snapshot(actor: &ActorId, snapshot: &[u8]) -> CoreResult<LoroDocument> {
        let doc = LoroDoc::new();
        doc.import(snapshot)
            .map_err(|e| CoreError::Sync(format!("loro import sidecar: {e}")))?;
        // Re-stamp the local peer so edits made on this device after loading are attributed here.
        set_actor(&doc, actor)?;
        Ok(LoroDocument { doc })
    }
}

impl NoteCrdt for LoroDocument {
    fn backend(&self) -> &'static str {
        super::LORO_BACKEND
    }

    fn text(&self) -> String {
        self.doc.get_text(BODY_CONTAINER).to_string()
    }

    fn set_text(&mut self, new_text: &str) {
        let body = self.doc.get_text(BODY_CONTAINER);
        // `update` diffs against the current value, so an unchanged `new_text` records no op —
        // satisfying the idempotence/loop-prevention contract on the trait.
        if body
            .update(new_text, UpdateOptions::default())
            .map_err(|e| CoreError::Sync(format!("loro update text: {e}")))
            .is_ok()
        {
            self.doc.commit();
        }
    }

    fn snapshot(&self) -> CoreResult<Vec<u8>> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| CoreError::Sync(format!("loro export snapshot: {e}")))
    }

    fn merge(&mut self, snapshot: &[u8]) -> CoreResult<()> {
        self.doc
            .import(snapshot)
            .map_err(|e| CoreError::Sync(format!("loro merge sidecar: {e}")))?;
        self.doc.commit();
        Ok(())
    }
}

/// Factory for [`LoroDocument`]s.
pub struct LoroBackend;

impl CrdtBackend for LoroBackend {
    fn name(&self) -> &'static str {
        super::LORO_BACKEND
    }

    fn new_document(&self, actor: &ActorId, text: &str) -> Box<dyn NoteCrdt> {
        // A construction failure here would only come from a corrupt in-memory seed; fall back to
        // an empty doc rather than panic, keeping the factory infallible like the LWW one.
        match LoroDocument::new(actor, text) {
            Ok(d) => Box::new(d),
            Err(_) => Box::new(LoroDocument {
                doc: LoroDoc::new(),
            }),
        }
    }

    fn load_document(&self, actor: &ActorId, snapshot: &[u8]) -> CoreResult<Box<dyn NoteCrdt>> {
        Ok(Box::new(LoroDocument::from_snapshot(actor, snapshot)?))
    }
}

/// Loro identifies replicas by a `u64` peer id; our actor ids are ulids. Hash the ulid into the
/// peer space (masking the top bit, which Loro reserves) — collisions are astronomically unlikely
/// and only a peer id, not data identity.
fn set_actor(doc: &LoroDoc, actor: &ActorId) -> CoreResult<()> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    actor.as_str().hash(&mut hasher);
    let peer = hasher.finish() & ((1u64 << 63) - 1);
    doc.set_peer_id(peer)
        .map_err(|e| CoreError::Sync(format!("loro set peer: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_edits_to_different_regions_both_survive() {
        // The whole point of the fine-grained backend: two devices append different text and the
        // merge keeps both, where an LWW register would keep only one.
        let a_actor = ActorId::new("device-a");
        let b_actor = ActorId::new("device-b");

        let mut a = LoroDocument::new(&a_actor, "shared base.").unwrap();
        let base = a.snapshot().unwrap();
        let mut b = LoroDocument::from_snapshot(&b_actor, &base).unwrap();

        a.set_text("shared base. A-addition");
        b.set_text("B-prefix shared base.");

        let snap_b = b.snapshot().unwrap();
        let snap_a = a.snapshot().unwrap();
        a.merge(&snap_b).unwrap();
        b.merge(&snap_a).unwrap();

        // Convergence: both replicas agree, and both edits are present.
        assert_eq!(a.text(), b.text());
        assert!(a.text().contains("A-addition"));
        assert!(a.text().contains("B-prefix"));
    }

    #[test]
    fn snapshot_round_trips() {
        let actor = ActorId::new("a");
        let d = LoroDocument::new(&actor, "hello loro").unwrap();
        let reloaded = LoroDocument::from_snapshot(&actor, &d.snapshot().unwrap()).unwrap();
        assert_eq!(reloaded.text(), "hello loro");
    }
}
