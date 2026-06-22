//! Building a world's **overlay**: the pins (points) and regions (clickable areas) that link
//! notes and categories into one image-backed or geo world (spatial-worlds.md "Overlays").
//!
//! Sources of pins/regions, all resolved through [`resolve`](crate::spatial::resolve) and filtered
//! to the requested world:
//! - a note's frontmatter `location` (the whole note pinned to one spot);
//! - each inline `loc:` token in a note's body (a trip log referencing several sites);
//! - a category's `location` — a pin, or a **region** when the category also carries
//!   [`SpatialRegion`] geometry (the `#kitchen` room on a floorplan).
//!
//! Tokens that fail to resolve, or that resolve to a *different* world, are silently skipped: one
//! malformed `loc:` must never blank out the whole overlay.

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::markdown::dialect;
use crate::model::{Category, Note, SpatialRegion, World};
use crate::spatial::location::{ResolvedLocation, WorldPoint};
use crate::spatial::lookup::LocationLookup;
use crate::spatial::registry::WorldRegistry;
use crate::spatial::resolve::resolve_token;

/// What an overlay item links to. The UI dispatches on this: a note pin opens the note, a category
/// pin/region runs the category's filtered-sorted view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SpatialTarget {
    Note { id: String, title: String },
    Category { name: String },
}

/// A point on the overlay linking to a note or category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub target: SpatialTarget,
    pub point: WorldPoint,
}

/// A clickable area on the overlay for a category (a room on a floorplan). `anchor` is the
/// category's `location` point — a label/centroid hint to pair with the geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayRegion {
    pub category: String,
    pub region: SpatialRegion,
    pub anchor: WorldPoint,
}

/// Everything needed to draw one world's overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    pub world: World,
    pub pins: Vec<Pin>,
    pub regions: Vec<OverlayRegion>,
}

/// Build the overlay for `world_id` from the book's notes and categories.
pub fn build_overlay(
    world_id: &str,
    notes: &[Note],
    categories: &[Category],
    registry: &WorldRegistry,
    lookup: &LocationLookup,
) -> CoreResult<Overlay> {
    let world = registry
        .get(world_id)
        .ok_or_else(|| crate::error::CoreError::NotFound(format!("world '{world_id}'")))?
        .clone();

    let mut pins = Vec::new();
    let mut regions = Vec::new();

    for note in notes {
        for token in note_location_tokens(note) {
            if let Some(point) = resolve_in_world(&token, world_id, registry, lookup) {
                pins.push(Pin {
                    target: SpatialTarget::Note {
                        id: note.id.to_string(),
                        title: note.title.clone(),
                    },
                    point,
                });
            }
        }
    }

    for category in categories {
        let Some(token) = &category.location else {
            continue;
        };
        let Some(point) = resolve_in_world(token, world_id, registry, lookup) else {
            continue;
        };
        match &category.region {
            Some(region) => regions.push(OverlayRegion {
                category: category.name.clone(),
                region: region.clone(),
                anchor: point,
            }),
            None => pins.push(Pin {
                target: SpatialTarget::Category {
                    name: category.name.clone(),
                },
                point,
            }),
        }
    }

    Ok(Overlay {
        world,
        pins,
        regions,
    })
}

/// A note's location tokens: its frontmatter `location` plus every inline `loc:` in the body.
fn note_location_tokens(note: &Note) -> Vec<String> {
    let mut tokens = Vec::new();
    if let Some(loc) = &note.location {
        tokens.push(loc.clone());
    }
    tokens.extend(dialect::extract_locations(&note.body));
    tokens
}

/// Resolve a token and keep it only if it lands in the requested world. Unresolvable tokens and
/// other-world tokens yield `None`.
fn resolve_in_world(
    token: &str,
    world_id: &str,
    registry: &WorldRegistry,
    lookup: &LocationLookup,
) -> Option<WorldPoint> {
    match resolve_token(token, registry, lookup) {
        Ok(ResolvedLocation { world, point }) if world == world_id => Some(point),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;
    use crate::spatial::lookup::LookupEntry;

    fn floor_registry() -> WorldRegistry {
        WorldRegistry::new(vec![World::image(
            "firstfloor",
            "First Floor",
            "drawing-1",
            (1000, 800),
        )])
    }

    fn note_with(location: Option<&str>, body: &str) -> Note {
        let mut n = Note::new(ObjectType::Note, "site visit", "syllepsis_001");
        n.location = location.map(str::to_string);
        n.body = body.to_string();
        n
    }

    #[test]
    fn collects_note_level_and_inline_pins_for_the_world() {
        let notes = vec![note_with(
            Some("firstfloor/0.1,0.1"),
            "saw a crack here loc:firstfloor/0.8,0.9 and a leak loc:earth/47.6,-122.3",
        )];
        let overlay = build_overlay(
            "firstfloor",
            &notes,
            &[],
            &floor_registry(),
            &LocationLookup::new(),
        )
        .unwrap();
        // Two firstfloor pins (note-level + one inline); the earth inline token is filtered out.
        assert_eq!(overlay.pins.len(), 2);
        assert!(overlay
            .pins
            .iter()
            .all(|p| matches!(p.target, SpatialTarget::Note { .. })));
    }

    #[test]
    fn category_without_region_is_a_pin_with_region_is_an_area() {
        let mut pin_cat = Category::new("entry");
        pin_cat.location = Some("firstfloor/0.2,0.2".into());

        let mut region_cat = Category::new("kitchen");
        region_cat.location = Some("firstfloor/0.5,0.5".into());
        region_cat.region = Some(SpatialRegion::SvgElement {
            element_id: "kitchen".into(),
        });

        let overlay = build_overlay(
            "firstfloor",
            &[],
            &[pin_cat, region_cat],
            &floor_registry(),
            &LocationLookup::new(),
        )
        .unwrap();

        assert_eq!(overlay.pins.len(), 1);
        assert_eq!(
            overlay.pins[0].target,
            SpatialTarget::Category {
                name: "entry".into()
            }
        );
        assert_eq!(overlay.regions.len(), 1);
        assert_eq!(overlay.regions[0].category, "kitchen");
    }

    #[test]
    fn named_place_pins_resolve_through_lookup() {
        let mut lookup = LocationLookup::new();
        lookup.upsert(LookupEntry::new("loft", "firstfloor", 0.3, 0.4));
        let notes = vec![note_with(Some("@loft"), "")];
        let overlay = build_overlay("firstfloor", &notes, &[], &floor_registry(), &lookup).unwrap();
        assert_eq!(overlay.pins.len(), 1);
        assert_eq!(overlay.pins[0].point, WorldPoint::Plane { x: 0.3, y: 0.4 });
    }

    #[test]
    fn malformed_token_does_not_break_overlay() {
        let notes = vec![note_with(Some("firstfloor/not,a,coord"), "")];
        let overlay = build_overlay(
            "firstfloor",
            &notes,
            &[],
            &floor_registry(),
            &LocationLookup::new(),
        )
        .unwrap();
        assert!(overlay.pins.is_empty());
    }

    #[test]
    fn unknown_world_errors() {
        assert!(build_overlay(
            "atlantis",
            &[],
            &[],
            &floor_registry(),
            &LocationLookup::new()
        )
        .is_err());
    }
}
