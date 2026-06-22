//! Parsing of the `loc:` location grammar and the coordinate types it resolves to
//! (spatial-worlds.md "Coordinates in markdown").
//!
//! The grammar (the value after `loc:`, or a note/category `location` field):
//!
//! ```text
//! 47.6062,-122.3321          Earth lat,long (world defaults to `earth`)
//! earth/47.6062,-122.3321    explicit geo world
//! firstfloor/0.42,0.31       image-backed world, normalized x,y
//! @kitchen                   named place, resolved via the lookup table
//! the kitchen                bare place name (same lookup, plain-text form)
//! ```
//!
//! Parsing is **world-agnostic**: a numeric pair is ambiguous (lat/long vs normalized x/y) until
//! it is [resolved](crate::spatial::resolve) against the target world's [kind](crate::model::WorldKind).

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

/// The raw value of a `loc:` token before it is resolved against a world.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationValue {
    /// A numeric coordinate pair. Whether it means `lat,long` or normalized `x,y` depends on the
    /// resolved world's kind, so it stays an uninterpreted pair until then.
    Pair(f64, f64),
    /// A named place (`@kitchen` or a plain `the kitchen`) resolved through the lookup table.
    Named(String),
}

/// A parsed-but-unresolved `loc:` token: an optional world id plus its raw value.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLocation {
    /// `None` when the token omitted a world prefix. For a coordinate pair that means the default
    /// world (`earth`); for a named place the world comes from the lookup-table row.
    pub world: Option<String>,
    pub value: LocationValue,
}

/// A coordinate resolved against its world's kind. This is the shape the overlay/UI consume.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WorldPoint {
    /// Latitude/longitude in degrees on a geo world.
    Geo { lat: f64, lon: f64 },
    /// Normalized `0..=1` position on an image-backed world.
    Plane { x: f64, y: f64 },
}

/// A fully resolved location: a concrete world id plus a coordinate in that world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLocation {
    pub world: String,
    pub point: WorldPoint,
}

/// Parse a raw `loc:` token value into an unresolved [`ParsedLocation`].
///
/// A leading `world/` segment sets the world; a `@` prefix or any non-numeric remainder is a named
/// place; otherwise the remainder must be a `a,b` numeric pair. Resolution into concrete
/// coordinates happens later against the world registry and lookup table.
pub fn parse_location(token: &str) -> CoreResult<ParsedLocation> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::parse("loc token", "empty location token"));
    }

    // A leading `world/` segment is optional. Split on the first '/' only so a coordinate's sign
    // or a multi-word place name after the prefix is preserved.
    let (world, rest) = match token.split_once('/') {
        Some((w, r)) if !w.is_empty() => (Some(w.trim().to_string()), r.trim()),
        _ => (None, token),
    };

    let value = if let Some(name) = rest.strip_prefix('@') {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::parse("loc token", "empty `@` place name"));
        }
        LocationValue::Named(name.to_string())
    } else if let Some(pair) = try_parse_pair(rest) {
        LocationValue::Pair(pair.0, pair.1)
    } else {
        // A non-numeric remainder is a plain-text place name (object-types.md "simplest form").
        LocationValue::Named(rest.to_string())
    };

    Ok(ParsedLocation { world, value })
}

/// Parse `a,b` into a float pair, or `None` if it isn't a clean numeric pair (so the caller can
/// fall back to treating it as a place name).
fn try_parse_pair(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.split_once(',')?;
    let a: f64 = a.trim().parse().ok()?;
    let b: f64 = b.trim().parse().ok()?;
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_WORLD_ID;

    #[test]
    fn parses_default_world_geo_pair() {
        let p = parse_location("47.6062,-122.3321").unwrap();
        assert_eq!(p.world, None);
        assert_eq!(p.value, LocationValue::Pair(47.6062, -122.3321));
    }

    #[test]
    fn parses_explicit_geo_and_image_worlds() {
        let geo = parse_location("earth/47.6,-122.3").unwrap();
        assert_eq!(geo.world.as_deref(), Some(DEFAULT_WORLD_ID));
        assert_eq!(geo.value, LocationValue::Pair(47.6, -122.3));

        let img = parse_location("firstfloor/0.42,0.31").unwrap();
        assert_eq!(img.world.as_deref(), Some("firstfloor"));
        assert_eq!(img.value, LocationValue::Pair(0.42, 0.31));
    }

    #[test]
    fn parses_named_places_at_and_plain() {
        assert_eq!(
            parse_location("@kitchen").unwrap().value,
            LocationValue::Named("kitchen".into())
        );
        // Bare plain-text place name (note frontmatter "simplest form").
        let plain = parse_location("the kitchen").unwrap();
        assert_eq!(plain.world, None);
        assert_eq!(plain.value, LocationValue::Named("the kitchen".into()));
        // Named place can still be world-qualified.
        let q = parse_location("firstfloor/@kitchen").unwrap();
        assert_eq!(q.world.as_deref(), Some("firstfloor"));
        assert_eq!(q.value, LocationValue::Named("kitchen".into()));
    }

    #[test]
    fn rejects_empty_tokens() {
        assert!(parse_location("   ").is_err());
        assert!(parse_location("earth/@").is_err());
    }
}
