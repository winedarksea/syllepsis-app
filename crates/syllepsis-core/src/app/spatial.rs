//! Application command surface for spatial worlds & overlays (spatial-worlds.md).
//!
//! Like the rest of [`crate::app`], these are framework-agnostic functions over a [`Book`] that
//! the Tauri shell wraps as commands. They assemble the [`WorldRegistry`] and lookup table from
//! the book's stored worlds, then resolve `loc:` tokens and build a world's overlay of pins and
//! regions over its (image or geo) backdrop.

use crate::error::{CoreError, CoreResult};
use crate::model::{Note, World, DEFAULT_WORLD_ID};
use crate::spatial::{
    build_overlay, resolve_token, LookupEntry, Overlay, ResolvedLocation, WorldRegistry,
};
use crate::storage::{Book, NoteStore};

/// Build the world registry for a book (its stored worlds plus the implicit `earth`).
fn registry_for(book: &Book) -> CoreResult<WorldRegistry> {
    Ok(WorldRegistry::new(book.store.worlds()?))
}

/// All worlds available in the book, `earth` first.
pub fn list_worlds(book: &Book) -> CoreResult<Vec<World>> {
    Ok(registry_for(book)?.worlds().to_vec())
}

/// Create or overwrite a world. The implicit `earth` world cannot be redefined as a stored file.
pub fn create_world(book: &Book, world: World) -> CoreResult<()> {
    if world.id == DEFAULT_WORLD_ID {
        return Err(CoreError::parse(
            "world",
            "'earth' is the built-in default world and cannot be redefined",
        ));
    }
    book.store.write_world(&world)
}

/// Delete a stored world. `earth` is built-in and not deletable.
pub fn delete_world(book: &Book, id: &str) -> CoreResult<()> {
    if id == DEFAULT_WORLD_ID {
        return Err(CoreError::parse(
            "world",
            "'earth' is the built-in default world and cannot be deleted",
        ));
    }
    book.store.delete_world(id)
}

/// Build the overlay (pins + regions) for one world from the book's visible notes and categories.
/// Archived and pending-deletion notes are excluded so they never surface on a map/floorplan.
pub fn world_overlay(book: &Book, world_id: &str) -> CoreResult<Overlay> {
    let notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| {
            !n.metadata.lifecycle.archived && n.metadata.lifecycle.marked_for_deletion_at.is_none()
        })
        .collect();
    let categories = book.store.categories()?;
    let registry = registry_for(book)?;
    let lookup = book.store.read_location_lookup()?;
    build_overlay(world_id, &notes, &categories, &registry, &lookup)
}

/// Every row of the text→coordinate lookup table.
pub fn location_lookup(book: &Book) -> CoreResult<Vec<LookupEntry>> {
    Ok(book.store.read_location_lookup()?.entries().to_vec())
}

/// Insert or replace one lookup-table entry (keyed on name + world), persisting the table.
pub fn set_location_lookup_entry(book: &Book, entry: LookupEntry) -> CoreResult<()> {
    let mut lookup = book.store.read_location_lookup()?;
    lookup.upsert(entry);
    book.store.write_location_lookup(&lookup)
}

/// Resolve a raw `loc:` token against the book's worlds and lookup table — backs the location
/// picker and inline-token rendering.
pub fn resolve_location(book: &Book, token: &str) -> CoreResult<ResolvedLocation> {
    let registry = registry_for(book)?;
    let lookup = book.store.read_location_lookup()?;
    resolve_token(token, &registry, &lookup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_category, create_note, update_note};
    use crate::model::{Category, ObjectType, SpatialRegion};
    use crate::spatial::{SpatialTarget, WorldPoint};
    use crate::storage::Book;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    #[test]
    fn list_worlds_always_includes_earth() {
        let (_d, book) = book();
        assert_eq!(list_worlds(&book).unwrap(), vec![World::earth()]);

        create_world(
            &book,
            World::image("firstfloor", "First Floor", "drawing-1", (1000, 800)),
        )
        .unwrap();
        let worlds = list_worlds(&book).unwrap();
        assert_eq!(worlds.len(), 2);
        assert!(worlds.iter().any(|w| w.id == "firstfloor"));
    }

    #[test]
    fn earth_is_protected_from_create_and_delete() {
        let (_d, book) = book();
        assert!(create_world(&book, World::earth()).is_err());
        assert!(delete_world(&book, DEFAULT_WORLD_ID).is_err());
    }

    #[test]
    fn overlay_assembles_note_pins_and_category_regions() {
        let (_d, book) = book();
        create_world(
            &book,
            World::image("firstfloor", "First Floor", "drawing-1", (1000, 800)),
        )
        .unwrap();

        // A note pinned to the floor, plus one with an inline loc: token in its body.
        let mut n = create_note(&book, ObjectType::Note, "leak", None).unwrap();
        n.location = Some("firstfloor/0.2,0.3".into());
        n.body = "and another spot loc:firstfloor/0.6,0.7".into();
        update_note(&book, n).unwrap();

        // A category rendered as a clickable region.
        let mut kitchen = Category::new("kitchen");
        kitchen.location = Some("firstfloor/0.5,0.5".into());
        kitchen.region = Some(SpatialRegion::SvgElement {
            element_id: "kitchen".into(),
        });
        create_category(&book, kitchen).unwrap();

        let overlay = world_overlay(&book, "firstfloor").unwrap();
        assert_eq!(overlay.world.id, "firstfloor");
        assert_eq!(overlay.pins.len(), 2);
        assert!(overlay
            .pins
            .iter()
            .all(|p| matches!(p.target, SpatialTarget::Note { .. })));
        assert_eq!(overlay.regions.len(), 1);
        assert_eq!(overlay.regions[0].category, "kitchen");
    }

    #[test]
    fn lookup_drives_named_place_resolution() {
        let (_d, book) = book();
        create_world(
            &book,
            World::image("firstfloor", "First Floor", "drawing-1", (1000, 800)),
        )
        .unwrap();
        set_location_lookup_entry(&book, LookupEntry::new("loft", "firstfloor", 0.3, 0.4)).unwrap();

        assert_eq!(location_lookup(&book).unwrap().len(), 1);
        let resolved = resolve_location(&book, "@loft").unwrap();
        assert_eq!(resolved.world, "firstfloor");
        assert_eq!(resolved.point, WorldPoint::Plane { x: 0.3, y: 0.4 });
    }

    #[test]
    fn resolve_plain_earth_coordinate_without_world_setup() {
        let (_d, book) = book();
        let resolved = resolve_location(&book, "47.6,-122.3").unwrap();
        assert_eq!(resolved.world, DEFAULT_WORLD_ID);
        assert_eq!(resolved.point, WorldPoint::Geo { lat: 47.6, lon: -122.3 });
    }
}
