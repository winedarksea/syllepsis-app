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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateImageWorldRequest {
    pub display_name: String,
    pub backdrop_asset_uuid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldDeletionImpact {
    pub note_references: usize,
    pub category_references: usize,
    pub lookup_references: usize,
}

impl WorldDeletionImpact {
    pub fn has_references(&self) -> bool {
        self.note_references > 0 || self.category_references > 0 || self.lookup_references > 0
    }
}

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

/// Create a validated image-backed world from an existing first-class Picture/Drawing asset.
pub fn create_image_world(book: &Book, request: CreateImageWorldRequest) -> CoreResult<World> {
    let display_name = request.display_name.trim();
    if display_name.is_empty() {
        return Err(CoreError::parse("world", "display name is required"));
    }
    let worlds = registry_for(book)?.worlds().to_vec();
    if worlds
        .iter()
        .any(|world| world.display_name.eq_ignore_ascii_case(display_name))
    {
        return Err(CoreError::parse(
            "world",
            format!("a world named '{display_name}' already exists"),
        ));
    }

    let matching_asset = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|note| {
            matches!(
                note.object_type,
                crate::model::ObjectType::Picture | crate::model::ObjectType::Drawing
            )
        })
        .find_map(|note| {
            note.asset
                .filter(|asset| asset.uuid == request.backdrop_asset_uuid)
        })
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "Picture/Drawing asset '{}'",
                request.backdrop_asset_uuid
            ))
        })?;

    let Some((actual_object_type, actual_dimensions, _media_type)) =
        crate::app::image_assets::inspect_tracked_asset(book, &matching_asset.uuid)?
    else {
        return Err(CoreError::NotFound(format!(
            "asset file '{}'",
            matching_asset.uuid
        )));
    };
    if !matches!(
        actual_object_type,
        crate::model::ObjectType::Picture | crate::model::ObjectType::Drawing
    ) {
        return Err(CoreError::parse(
            "world",
            "backdrop asset is not a supported Picture or Drawing",
        ));
    }

    let base_id = world_slug(display_name);
    let mut id = base_id.clone();
    let mut suffix = 2_u32;
    while id == DEFAULT_WORLD_ID || worlds.iter().any(|world| world.id == id) {
        id = format!("{base_id}-{suffix}");
        suffix += 1;
    }
    let world = World::image(id, display_name, matching_asset.uuid, actual_dimensions);
    book.store.write_world(&world)?;
    Ok(world)
}

/// Count every persisted reference that would become invalid if a world were deleted.
pub fn world_deletion_impact(book: &Book, id: &str) -> CoreResult<WorldDeletionImpact> {
    if id == DEFAULT_WORLD_ID {
        return Err(CoreError::parse("world", "'earth' cannot be deleted"));
    }
    // Deletion protection must include archived and pending-deletion notes even though the normal
    // visual overlay hides them. A hidden reference is still user data that would be orphaned.
    let notes = book.store.read_all_notes()?;
    let categories = book.store.categories()?;
    let registry = registry_for(book)?;
    let lookup = book.store.read_location_lookup()?;
    let overlay = build_overlay(id, &notes, &categories, &registry, &lookup)?;
    let note_references = overlay
        .pins
        .iter()
        .filter(|pin| matches!(pin.target, crate::spatial::SpatialTarget::Note { .. }))
        .count();
    let category_point_references = overlay
        .pins
        .iter()
        .filter(|pin| matches!(pin.target, crate::spatial::SpatialTarget::Category { .. }))
        .count();
    Ok(WorldDeletionImpact {
        note_references,
        category_references: category_point_references + overlay.regions.len(),
        lookup_references: lookup
            .entries()
            .iter()
            .filter(|entry| entry.world == id)
            .count(),
    })
}

/// Delete a stored world. `earth` is built-in and not deletable.
pub fn delete_world(book: &Book, id: &str) -> CoreResult<()> {
    if id == DEFAULT_WORLD_ID {
        return Err(CoreError::parse(
            "world",
            "'earth' is the built-in default world and cannot be deleted",
        ));
    }
    let impact = world_deletion_impact(book, id)?;
    if impact.has_references() {
        return Err(CoreError::parse(
            "world",
            format!(
                "world '{id}' is still referenced by {} note location(s), {} category location(s), and {} lookup entry/entries",
                impact.note_references, impact.category_references, impact.lookup_references
            ),
        ));
    }
    book.store.delete_world(id)
}

fn world_slug(display_name: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;
    for character in display_name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_separator = false;
        } else if !slug.is_empty() && !previous_was_separator {
            slug.push('-');
            previous_was_separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "world".to_string()
    } else {
        slug
    }
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
    let Some((absolute, mime)) = crate::app::image_assets::asset_file(book, &backdrop_uuid)? else {
        return Ok(None);
    };
    Ok(Some(BackdropRef {
        mime,
        path: absolute.to_string_lossy().into_owned(),
    }))
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
        std::fs::write(
            book.root.join("floorplan.svg"),
            br#"<svg viewBox="0 0 1000 800"/>"#,
        )
        .unwrap();
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

    #[test]
    fn validated_image_world_creation_derives_unique_slug_and_dimensions() {
        let (directory, book) = book();
        let source = directory.path().join("floor.svg");
        std::fs::write(
            &source,
            r#"<svg viewBox="0 0 900 600"><path id="hall" d="M0 0"/></svg>"#,
        )
        .unwrap();
        let image = crate::app::image_assets::import_image_object(
            &book,
            source.to_str().unwrap(),
            Some("Floor plan"),
        )
        .unwrap();
        let asset_uuid = image.asset.unwrap().uuid;

        let world = create_image_world(
            &book,
            CreateImageWorldRequest {
                display_name: "First Floor".into(),
                backdrop_asset_uuid: asset_uuid,
            },
        )
        .unwrap();
        assert_eq!(world.id, "first-floor");
        assert_eq!(world.intrinsic_dimensions, Some((900, 600)));

        let duplicate_name = create_image_world(
            &book,
            CreateImageWorldRequest {
                display_name: "first floor".into(),
                backdrop_asset_uuid: world.backdrop.unwrap(),
            },
        );
        assert!(duplicate_name.is_err());
    }

    #[test]
    fn deletion_is_blocked_while_locations_reference_world() {
        let (directory, book) = book();
        let source = directory.path().join("map.svg");
        std::fs::write(&source, r#"<svg viewBox="0 0 10 10"/>"#).unwrap();
        let image = crate::app::image_assets::import_image_object(
            &book,
            source.to_str().unwrap(),
            Some("Map"),
        )
        .unwrap();
        let world = create_image_world(
            &book,
            CreateImageWorldRequest {
                display_name: "Referenced".into(),
                backdrop_asset_uuid: image.asset.unwrap().uuid,
            },
        )
        .unwrap();
        let mut note = create_note(&book, ObjectType::Note, "Pinned", None).unwrap();
        note.location = Some(format!("{}/0.2,0.3", world.id));
        note.metadata.lifecycle.archived = true;
        update_note(&book, note).unwrap();

        let impact = world_deletion_impact(&book, &world.id).unwrap();
        assert_eq!(impact.note_references, 1);
        assert!(delete_world(&book, &world.id).is_err());
    }
}
