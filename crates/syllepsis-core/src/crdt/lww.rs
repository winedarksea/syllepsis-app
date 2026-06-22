//! The built-in, always-compiled CRDT backend: a **last-writer-wins register** over the whole
//! note body, keyed by a hybrid logical clock (HLC).
//!
//! An LWW-register is a genuine CRDT: its merge is `max` over a *total* order on the clock, which
//! is commutative, associative, and idempotent, so replicas converge regardless of sync order or
//! duplication. The clock is `(wall_ms, counter, actor)`:
//! - `wall_ms` — wall-clock milliseconds, so a later real edit wins (the user's intuition).
//! - `counter` — a Lamport counter that breaks ties and keeps the clock monotonic across merges
//!   even when two devices' wall clocks read the same millisecond or run backwards.
//! - `actor` — the device id, the final deterministic tie-break so two devices never disagree.
//!
//! This is the right default for the stated primary target — one user, several devices, rarely
//! *truly* concurrent on the same note. It does not merge concurrent edits to different paragraphs
//! the way the optional Loro backend does; it picks one whole-body winner. That is a real, honest
//! convergence strategy (the one Git LFS pointers, Drive, and many sync systems use), not a stub.

use serde::{Deserialize, Serialize};

use crate::crdt::{ActorId, CrdtBackend, NoteCrdt};
use crate::error::{CoreError, CoreResult};

/// A hybrid logical clock stamping each register write. Ordered as the tuple
/// `(wall_ms, counter, actor)` — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Hlc {
    wall_ms: u64,
    counter: u64,
    actor: String,
}

impl Hlc {
    /// Total-order key. Deliberately by value/borrow so it can feed `max`/`cmp` directly.
    fn key(&self) -> (u64, u64, &str) {
        (self.wall_ms, self.counter, self.actor.as_str())
    }

    /// Advance for a *local* write at wall time `now_ms`. Monotonic: time never goes backwards
    /// (a backwards wall clock keeps the previous `wall_ms` and just bumps the counter).
    fn tick_local(&self, now_ms: u64, actor: &str) -> Hlc {
        if now_ms > self.wall_ms {
            Hlc {
                wall_ms: now_ms,
                counter: 0,
                actor: actor.to_string(),
            }
        } else {
            Hlc {
                wall_ms: self.wall_ms,
                counter: self.counter + 1,
                actor: actor.to_string(),
            }
        }
    }
}

/// The on-disk sidecar shape: the winning text and the clock that stamped it. JSON so the sidecar
/// is inspectable and, crucially, serializes to **identical bytes for identical state** (fixed
/// field order, no maps) — what lets the sync engine detect "nothing changed" and avoid loops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LwwSnapshot {
    text: String,
    clock: Hlc,
}

/// A note body as an LWW register. See the module docs.
pub struct LwwTextDocument {
    actor: String,
    text: String,
    clock: Hlc,
}

impl LwwTextDocument {
    fn new(actor: &ActorId, text: &str) -> LwwTextDocument {
        let actor = actor.as_str().to_string();
        let clock = Hlc {
            wall_ms: now_ms(),
            counter: 0,
            actor: actor.clone(),
        };
        LwwTextDocument {
            actor,
            text: text.to_string(),
            clock,
        }
    }

    fn from_snapshot(actor: &ActorId, snapshot: &[u8]) -> CoreResult<LwwTextDocument> {
        let parsed: LwwSnapshot = serde_json::from_slice(snapshot)
            .map_err(|e| CoreError::Sync(format!("decode lww sidecar: {e}")))?;
        Ok(LwwTextDocument {
            actor: actor.as_str().to_string(),
            text: parsed.text,
            clock: parsed.clock,
        })
    }

    /// Local edit at an explicit wall time. The public [`NoteCrdt::set_text`] calls this with the
    /// real clock; tests call it directly to make "which write wins" deterministic.
    pub(crate) fn set_text_at(&mut self, new_text: &str, now_ms: u64) {
        if new_text == self.text {
            return; // idempotent: re-applying the current value must not bump the clock.
        }
        self.clock = self.clock.tick_local(now_ms, &self.actor);
        self.text = new_text.to_string();
    }
}

impl NoteCrdt for LwwTextDocument {
    fn backend(&self) -> &'static str {
        super::LWW_BACKEND
    }

    fn text(&self) -> String {
        self.text.clone()
    }

    fn set_text(&mut self, new_text: &str) {
        self.set_text_at(new_text, now_ms());
    }

    fn snapshot(&self) -> CoreResult<Vec<u8>> {
        let snap = LwwSnapshot {
            text: self.text.clone(),
            clock: self.clock.clone(),
        };
        serde_json::to_vec(&snap).map_err(|e| CoreError::Sync(format!("encode lww sidecar: {e}")))
    }

    fn merge(&mut self, snapshot: &[u8]) -> CoreResult<()> {
        let other: LwwSnapshot = serde_json::from_slice(snapshot)
            .map_err(|e| CoreError::Sync(format!("decode lww sidecar: {e}")))?;
        // max over the total order: the larger clock's text wins, and both replicas adopt that same
        // (text, clock) so a subsequent local edit on either device supersedes it cleanly.
        if other.clock.key() > self.clock.key() {
            self.text = other.text;
            self.clock = other.clock;
        }
        Ok(())
    }
}

/// Factory for [`LwwTextDocument`]s.
pub struct LwwBackend;

impl CrdtBackend for LwwBackend {
    fn name(&self) -> &'static str {
        super::LWW_BACKEND
    }

    fn new_document(&self, actor: &ActorId, text: &str) -> Box<dyn NoteCrdt> {
        Box::new(LwwTextDocument::new(actor, text))
    }

    fn load_document(&self, actor: &ActorId, snapshot: &[u8]) -> CoreResult<Box<dyn NoteCrdt>> {
        Ok(Box::new(LwwTextDocument::from_snapshot(actor, snapshot)?))
    }
}

/// Wall-clock milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(actor: &str, text: &str) -> LwwTextDocument {
        LwwTextDocument::new(&ActorId::new(actor), text)
    }

    /// A snapshot with a fully-controlled clock — used to test merge order-independence without
    /// the real wall clock (`now_ms`) leaking into the comparison.
    fn snapshot_at(actor: &str, text: &str, wall_ms: u64, counter: u64) -> Vec<u8> {
        serde_json::to_vec(&LwwSnapshot {
            text: text.to_string(),
            clock: Hlc {
                wall_ms,
                counter,
                actor: actor.to_string(),
            },
        })
        .unwrap()
    }

    /// A document whose clock is pinned to an explicit wall time, so tests are independent of when
    /// the document happened to be constructed.
    fn doc_at(actor: &str, text: &str, wall_ms: u64) -> LwwTextDocument {
        let mut d = doc(actor, text);
        d.clock = Hlc {
            wall_ms,
            counter: 0,
            actor: actor.to_string(),
        };
        d
    }

    #[test]
    fn snapshot_round_trips_text() {
        let d = doc("a", "hello world");
        let snap = d.snapshot().unwrap();
        let back = LwwTextDocument::from_snapshot(&ActorId::new("a"), &snap).unwrap();
        assert_eq!(back.text(), "hello world");
    }

    #[test]
    fn setting_the_same_text_does_not_bump_the_clock() {
        // Loop-prevention contract: equal text → identical snapshot bytes.
        let mut d = doc("a", "same");
        let before = d.snapshot().unwrap();
        d.set_text("same");
        assert_eq!(d.snapshot().unwrap(), before);
    }

    #[test]
    fn later_local_write_wins_over_earlier() {
        let mut a = doc("a", "base");
        let mut b = doc("b", "base");
        a.set_text_at("from-a", 1_000);
        b.set_text_at("from-b", 2_000); // later wall time

        // Merge in both directions; both replicas must converge on the later write ("from-b").
        let snap_b = b.snapshot().unwrap();
        let snap_a = a.snapshot().unwrap();
        a.merge(&snap_b).unwrap();
        b.merge(&snap_a).unwrap();
        assert_eq!(a.text(), "from-b");
        assert_eq!(b.text(), "from-b");
        assert_eq!(a.snapshot().unwrap(), b.snapshot().unwrap());
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        // Two concurrent edits stamped at the *same* wall time and counter, so only the actor id
        // breaks the tie ("b" > "a"). Clocks are explicit so the comparison can't depend on when
        // these docs were constructed (the real `now_ms`).
        let sa = snapshot_at("a", "edit-a", 5_000, 1);
        let sb = snapshot_at("b", "edit-b", 5_000, 1);

        // Base docs sit at an earlier wall time, so both incoming edits supersede the base.
        let mut left = doc_at("a", "base", 1_000);
        left.merge(&sa).unwrap();
        left.merge(&sb).unwrap();
        left.merge(&sb).unwrap(); // idempotent: a second merge of the same snapshot changes nothing

        let mut right = doc_at("z", "base", 1_000);
        right.merge(&sb).unwrap();
        right.merge(&sa).unwrap();

        assert_eq!(left.text(), right.text(), "merge must be order-independent");
        assert_eq!(
            left.text(),
            "edit-b",
            "the actor tie-break ('b' > 'a') decides"
        );
        // Converged state is byte-identical regardless of merge order (loop-prevention contract).
        assert_eq!(left.snapshot().unwrap(), right.snapshot().unwrap());
    }
}
