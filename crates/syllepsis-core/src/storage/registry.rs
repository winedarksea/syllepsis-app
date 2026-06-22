//! Book-level id registry: the collision backstop described in object-types.md.
//!
//! ULIDs are 128-bit and time-ordered, so independent offline devices effectively never
//! collide. The registry exists for the *astronomically rare* hit: on creation, fork, and
//! pack-import, a candidate's ulid is checked here and regenerated if already present. It
//! also gives O(1) "does this identity exist" lookups for link resolution.
//!
//! The registry indexes ulids (the canonical identity tail), not full ids, so a slug that
//! drifts after a rename still maps to the same registered identity.

use std::collections::HashSet;

use crate::id::NoteId;

#[derive(Debug, Default, Clone)]
pub struct IdRegistry {
    ulids: HashSet<String>,
}

impl IdRegistry {
    /// Build a registry from an existing set of ids (e.g. from a store scan).
    pub fn from_ids<'a>(ids: impl IntoIterator<Item = &'a NoteId>) -> Self {
        let ulids = ids.into_iter().map(|id| id.ulid().to_string()).collect();
        IdRegistry { ulids }
    }

    /// True if this identity is already known to the book.
    pub fn contains(&self, id: &NoteId) -> bool {
        self.ulids.contains(id.ulid())
    }

    /// Record an identity. Returns `false` if it was already present.
    pub fn register(&mut self, id: &NoteId) -> bool {
        self.ulids.insert(id.ulid().to_string())
    }

    /// Forget an identity (e.g. after a permanent delete).
    pub fn remove(&mut self, id: &NoteId) {
        self.ulids.remove(id.ulid());
    }

    /// Mint a fresh, registered id whose ulid is guaranteed unique within this book,
    /// regenerating on the rare collision. Use for create and fork.
    pub fn mint(&mut self, type_prefix: &str, title: &str) -> NoteId {
        loop {
            let candidate = NoteId::generate(type_prefix, title);
            if self.register(&candidate) {
                return candidate;
            }
        }
    }

    /// Reconcile an externally-supplied id (knowledge-pack import): keep its ulid if free,
    /// otherwise mint a new identity preserving type and title. Returns the registered id.
    pub fn reconcile(&mut self, incoming: &NoteId) -> NoteId {
        if self.register(incoming) {
            incoming.clone()
        } else {
            self.mint(incoming.type_prefix(), incoming.slug())
        }
    }

    /// Number of registered identities.
    pub fn len(&self) -> usize {
        self.ulids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ulids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_and_detects() {
        let a = NoteId::generate("note", "a");
        let mut reg = IdRegistry::default();
        assert!(reg.register(&a));
        assert!(!reg.register(&a)); // already present
        assert!(reg.contains(&a));
    }

    #[test]
    fn mint_is_always_unique() {
        let mut reg = IdRegistry::default();
        let first = reg.mint("note", "same title");
        let second = reg.mint("note", "same title");
        assert_ne!(first.ulid(), second.ulid());
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn reconcile_keeps_free_ulid_but_remints_collision() {
        let incoming = NoteId::generate("note", "imported");
        let mut reg = IdRegistry::default();
        let kept = reg.reconcile(&incoming);
        assert_eq!(kept.ulid(), incoming.ulid()); // free → kept
        let remint = reg.reconcile(&incoming); // now collides → new identity
        assert_ne!(remint.ulid(), incoming.ulid());
    }
}
