//! Resolution of a parsed `loc:` token into a concrete coordinate, using the world registry (to
//! learn each world's coordinate system) and the lookup table (to resolve named places).
//!
//! This is the step that disambiguates a numeric pair: the same `0.42,0.31` is normalized `x,y`
//! in an image world but would be `lat,long` in a geo world. The world's
//! [kind](crate::model::WorldKind) decides, and the value is range-checked against that system.

use crate::error::{CoreError, CoreResult};
use crate::model::{WorldKind, DEFAULT_WORLD_ID};
use crate::spatial::location::{LocationValue, ParsedLocation, ResolvedLocation, WorldPoint};
use crate::spatial::lookup::LocationLookup;
use crate::spatial::registry::WorldRegistry;

// Geographic coordinate bounds in degrees, fixed by the lat/long system (not tunable config).
const LAT_MIN: f64 = -90.0;
const LAT_MAX: f64 = 90.0;
const LON_MIN: f64 = -180.0;
const LON_MAX: f64 = 180.0;
// Image-backed worlds store coordinates normalized to this inclusive range.
const NORM_MIN: f64 = 0.0;
const NORM_MAX: f64 = 1.0;

/// Resolve a parsed location against the book's worlds and lookup table.
pub fn resolve(
    parsed: &ParsedLocation,
    registry: &WorldRegistry,
    lookup: &LocationLookup,
) -> CoreResult<ResolvedLocation> {
    match &parsed.value {
        LocationValue::Named(name) => {
            let entry = lookup
                .resolve(name, parsed.world.as_deref())
                .ok_or_else(|| CoreError::NotFound(format!("location '{name}'")))?;
            let kind = registry.kind_of(&entry.world)?;
            let point = point_from_pair(kind, entry.first, entry.second)?;
            Ok(ResolvedLocation {
                world: entry.world.clone(),
                point,
            })
        }
        LocationValue::Pair(a, b) => {
            let world = parsed
                .world
                .clone()
                .unwrap_or_else(|| DEFAULT_WORLD_ID.to_string());
            let kind = registry.kind_of(&world)?;
            let point = point_from_pair(kind, *a, *b)?;
            Ok(ResolvedLocation { world, point })
        }
    }
}

/// Convenience: parse and resolve a raw token in one call.
pub fn resolve_token(
    token: &str,
    registry: &WorldRegistry,
    lookup: &LocationLookup,
) -> CoreResult<ResolvedLocation> {
    let parsed = crate::spatial::location::parse_location(token)?;
    resolve(&parsed, registry, lookup)
}

/// Interpret a numeric pair under a world's coordinate system, range-checking it.
fn point_from_pair(kind: WorldKind, a: f64, b: f64) -> CoreResult<WorldPoint> {
    match kind {
        WorldKind::Geo => {
            if !(LAT_MIN..=LAT_MAX).contains(&a) {
                return Err(CoreError::parse(
                    "location",
                    format!("latitude {a} out of range [{LAT_MIN}, {LAT_MAX}]"),
                ));
            }
            if !(LON_MIN..=LON_MAX).contains(&b) {
                return Err(CoreError::parse(
                    "location",
                    format!("longitude {b} out of range [{LON_MIN}, {LON_MAX}]"),
                ));
            }
            Ok(WorldPoint::Geo { lat: a, lon: b })
        }
        WorldKind::Image => {
            for (label, v) in [("x", a), ("y", b)] {
                if !(NORM_MIN..=NORM_MAX).contains(&v) {
                    return Err(CoreError::parse(
                        "location",
                        format!("normalized {label}={v} out of range [{NORM_MIN}, {NORM_MAX}]"),
                    ));
                }
            }
            Ok(WorldPoint::Plane { x: a, y: b })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::World;
    use crate::spatial::location::parse_location;
    use crate::spatial::lookup::LookupEntry;

    fn registry() -> WorldRegistry {
        WorldRegistry::new(vec![World::image(
            "firstfloor",
            "First Floor",
            "drawing-1",
            (1000, 800),
        )])
    }

    #[test]
    fn geo_pair_resolves_against_earth() {
        let p = parse_location("47.6,-122.3").unwrap();
        let r = resolve(&p, &registry(), &LocationLookup::new()).unwrap();
        assert_eq!(r.world, DEFAULT_WORLD_ID);
        assert_eq!(r.point, WorldPoint::Geo { lat: 47.6, lon: -122.3 });
    }

    #[test]
    fn image_pair_is_normalized_xy() {
        let r = resolve_token("firstfloor/0.42,0.31", &registry(), &LocationLookup::new()).unwrap();
        assert_eq!(r.world, "firstfloor");
        assert_eq!(r.point, WorldPoint::Plane { x: 0.42, y: 0.31 });
    }

    #[test]
    fn named_place_resolves_through_lookup() {
        let mut lookup = LocationLookup::new();
        lookup.upsert(LookupEntry::new("kitchen", "firstfloor", 0.5, 0.6));
        let r = resolve_token("@kitchen", &registry(), &lookup).unwrap();
        assert_eq!(r.world, "firstfloor");
        assert_eq!(r.point, WorldPoint::Plane { x: 0.5, y: 0.6 });
    }

    #[test]
    fn out_of_range_coordinates_error() {
        // Latitude past the pole.
        assert!(resolve_token("99.0,0.0", &registry(), &LocationLookup::new()).is_err());
        // Normalized coordinate above 1.0 in an image world.
        assert!(resolve_token("firstfloor/1.2,0.3", &registry(), &LocationLookup::new()).is_err());
    }

    #[test]
    fn unknown_world_and_unknown_place_error() {
        assert!(resolve_token("atlantis/0.1,0.2", &registry(), &LocationLookup::new()).is_err());
        assert!(resolve_token("@nowhere", &registry(), &LocationLookup::new()).is_err());
    }
}
