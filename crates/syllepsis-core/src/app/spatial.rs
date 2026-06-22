//! Application command surface for spatial worlds & overlays (spatial-worlds.md).
//!
//! Like the rest of [`crate::app`], these are framework-agnostic functions over a [`Book`] that
//! the Tauri shell wraps as commands. They assemble the [`WorldRegistry`] and lookup table from
//! the book's stored worlds, then resolve `loc:` tokens and build a world's overlay of pins and
//! regions over its (image or geo) backdrop.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::model::{Note, World, WorldKind, DEFAULT_WORLD_ID};
use crate::spatial::{
    build_overlay, resolve_token, LookupEntry, Overlay, ResolvedLocation, WorldRegistry,
};
use crate::storage::{Book, NoteStore};
use crate::sync::AssetRegistry;

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
        .filter(|n| n.metadata.is_visible_in_default_views())
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

/// The resolved on-disk backdrop of an image-backed world: the asset's absolute path plus the MIME
/// type inferred from its extension. The shell reads the file and serves it to the overlay view
/// (spatial-worlds.md — the floorplan/mind-palace backdrop the pins and regions sit over).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackdropRef {
    pub path: String,
    pub mime: String,
}

/// Resolve a world's backdrop asset to a concrete file. Returns `None` (not an error) when the
/// world is geo, has no backdrop set, or its backdrop UUID is not tracked by any asset on disk yet
/// — all normal "nothing to draw behind the overlay" states the view renders as a placeholder.
pub fn world_backdrop(book: &Book, world_id: &str) -> CoreResult<Option<BackdropRef>> {
    let Some(world) = registry_for(book)?.get(world_id).cloned() else {
        return Err(CoreError::NotFound(format!("world '{world_id}'")));
    };
    if world.kind != WorldKind::Image {
        return Ok(None);
    }
    let Some(backdrop_uuid) = world.backdrop else {
        return Ok(None);
    };
    let registry = AssetRegistry::scan(&book.root)?;
    let Some(relative_path) = registry.resolve(&backdrop_uuid) else {
        return Ok(None);
    };
    let absolute = book.root.join(relative_path);
    Ok(Some(BackdropRef {
        mime: mime_for_path(relative_path).to_string(),
        path: absolute.to_string_lossy().into_owned(),
    }))
}

/// MIME type for a backdrop image/drawing from its extension (the kinds `sync::assets` tracks).
fn mime_for_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
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
        assert_eq!(
            resolved.point,
            WorldPoint::Geo {
                lat: 47.6,
                lon: -122.3
            }
        );
    }

    #[test]
    fn backdrop_resolves_a_tracked_asset_and_skips_geo() {
        let (_d, book) = book();
        // Geo world (earth) has no backdrop.
        assert_eq!(world_backdrop(&book, DEFAULT_WORLD_ID).unwrap(), None);

        // An image world whose backdrop UUID is not yet on disk → None (placeholder state).
        create_world(
            &book,
            World::image("firstfloor", "First Floor", "missing-uuid", (1000, 800)),
        )
        .unwrap();
        assert_eq!(world_backdrop(&book, "firstfloor").unwrap(), None);

        // Drop a real SVG asset, track it by UUID, and point a world at it.
        std::fs::write(book.root.join("floorplan.svg"), b"<svg/>").unwrap();
        let uuid = crate::sync::assign_asset_uuid(&book.root, "floorplan.svg").unwrap();
        create_world(
            &book,
            World::image("floor2", "Second Floor", &uuid, (1000, 800)),
        )
        .unwrap();

        let backdrop = world_backdrop(&book, "floor2").unwrap().unwrap();
        assert!(backdrop.path.ends_with("floorplan.svg"));
        assert_eq!(backdrop.mime, "image/svg+xml");

        // Unknown world id is an error, not None.
        assert!(world_backdrop(&book, "nope").is_err());
    }
}
