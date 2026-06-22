//! The in-memory world registry: the set of worlds a book knows about, with `earth` always
//! present as the implicit default geo world (spatial-worlds.md "Worlds registry").
//!
//! Persistence of the individual [`World`] entries lives in [`crate::storage`] (one frontmatter
//! file per world under `_worlds/`, mirroring `_categories/`); this type is the resolved view the
//! [`resolve`](crate::spatial::resolve) and [`overlay`](crate::spatial::overlay) layers query.

use crate::error::{CoreError, CoreResult};
use crate::model::{World, WorldKind, DEFAULT_WORLD_ID};

/// All worlds available to a book. `earth` is synthesized when absent so a book that never defined
/// a world can still resolve plain `loc:lat,long` tokens.
#[derive(Debug, Clone)]
pub struct WorldRegistry {
    worlds: Vec<World>,
}

impl WorldRegistry {
    /// Build a registry from the worlds stored in a book, guaranteeing the implicit `earth` world.
    pub fn new(mut worlds: Vec<World>) -> WorldRegistry {
        if !worlds.iter().any(|w| w.id == DEFAULT_WORLD_ID) {
            worlds.insert(0, World::earth());
        }
        WorldRegistry { worlds }
    }

    /// A registry with only the implicit `earth` world.
    pub fn earth_only() -> WorldRegistry {
        WorldRegistry::new(Vec::new())
    }

    /// Look up a world by id.
    pub fn get(&self, id: &str) -> Option<&World> {
        self.worlds.iter().find(|w| w.id == id)
    }

    /// The kind of a world, erroring if the id is unknown (so a `loc:` token referencing a
    /// nonexistent world fails loudly rather than guessing a coordinate system).
    pub fn kind_of(&self, id: &str) -> CoreResult<WorldKind> {
        self.get(id)
            .map(|w| w.kind)
            .ok_or_else(|| CoreError::NotFound(format!("world '{id}'")))
    }

    /// All worlds, `earth` first.
    pub fn worlds(&self) -> &[World] {
        &self.worlds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_is_always_present() {
        let reg = WorldRegistry::earth_only();
        assert_eq!(reg.worlds().len(), 1);
        assert_eq!(reg.kind_of(DEFAULT_WORLD_ID).unwrap(), WorldKind::Geo);
    }

    #[test]
    fn custom_worlds_join_earth() {
        let floor = World::image("firstfloor", "First Floor", "drawing-1", (1000, 800));
        let reg = WorldRegistry::new(vec![floor]);
        assert_eq!(reg.worlds().len(), 2);
        assert_eq!(reg.kind_of("firstfloor").unwrap(), WorldKind::Image);
        assert_eq!(reg.kind_of(DEFAULT_WORLD_ID).unwrap(), WorldKind::Geo);
    }

    #[test]
    fn a_stored_earth_is_not_duplicated() {
        let reg = WorldRegistry::new(vec![World::earth()]);
        assert_eq!(reg.worlds().len(), 1);
    }

    #[test]
    fn unknown_world_errors() {
        assert!(WorldRegistry::earth_only().kind_of("atlantis").is_err());
    }
}
