//! Spatial worlds registry (spatial-worlds.md). A **world** is a coordinate space a note can
//! be placed in: either *geo* (lat/long on a sphere) or *image-backed* (a 2D plane over a
//! drawing/raster backdrop, with normalized `0..1` coordinates so locations survive the
//! backdrop being re-exported at a new resolution).
//!
//! The registry types ship now; the picker UI, overlays, and the text→coordinate lookup
//! table are wired up in the spatial phase. `earth` is the implicit default geo world.

use serde::{Deserialize, Serialize};

/// The default geo world id used when a `loc:` token omits its world.
pub const DEFAULT_WORLD_ID: &str = "earth";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldKind {
    /// lat/long on a sphere; backed by map tiles (future).
    Geo,
    /// 2D plane over an image backdrop; normalized `(x, y)` coordinates.
    Image,
}

/// A registry entry describing one world. Stored alongside `_categories/` in the book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub id: String,
    pub display_name: String,
    pub kind: WorldKind,
    /// For image worlds: UUID reference to the backing drawing/image object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<String>,
    /// For image worlds: the backdrop's intrinsic pixel dimensions (width, height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intrinsic_dimensions: Option<(u32, u32)>,
    /// For geo worlds: an optional map-tile source URL (absent = no tiles, e.g. fantasy maps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_source: Option<String>,
}

impl World {
    /// The built-in Earth geo world.
    pub fn earth() -> World {
        World {
            id: DEFAULT_WORLD_ID.to_string(),
            display_name: "Earth".to_string(),
            kind: WorldKind::Geo,
            backdrop: None,
            intrinsic_dimensions: None,
            tile_source: None,
        }
    }

    /// Construct an image-backed world over the given backdrop object.
    pub fn image(
        id: impl Into<String>,
        display_name: impl Into<String>,
        backdrop: impl Into<String>,
        intrinsic_dimensions: (u32, u32),
    ) -> World {
        World {
            id: id.into(),
            display_name: display_name.into(),
            kind: WorldKind::Image,
            backdrop: Some(backdrop.into()),
            intrinsic_dimensions: Some(intrinsic_dimensions),
            tile_source: None,
        }
    }
}

/// A clickable **area** (not just a point) in an image-backed world (spatial-worlds.md
/// "Overlays"). This is what turns `#kitchen` into a clickable room on a floorplan. Geometry is
/// stored in **normalized** `0..=1` coordinates of the backdrop's intrinsic size — the same frame
/// points use — so a region survives the backdrop being re-exported at a different resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "shape")]
pub enum SpatialRegion {
    /// A named element in an SVG/drawing backdrop (`id="kitchen"`). The vector element *is* the
    /// region, so an imported SVG floorplan gets clickable rooms essentially for free; no stored
    /// geometry is needed because the SVG already carries it.
    SvgElement { element_id: String },
    /// A normalized axis-aligned bounding box: top-left `(x, y)` plus `width`/`height`, each in
    /// `0..=1`. The fallback for raster backdrops, which have no named elements.
    BoundingBox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// A normalized polygon (`0..=1` vertices) for non-rectangular raster regions.
    Polygon { points: Vec<(f64, f64)> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_is_geo_default() {
        let earth = World::earth();
        assert_eq!(earth.id, DEFAULT_WORLD_ID);
        assert_eq!(earth.kind, WorldKind::Geo);
        let back: World = serde_yaml::from_str(&serde_yaml::to_string(&earth).unwrap()).unwrap();
        assert_eq!(earth, back);
    }

    #[test]
    fn image_world_carries_backdrop() {
        let w = World::image("firstfloor", "First Floor", "drawing-uuid", (1024, 768));
        assert_eq!(w.kind, WorldKind::Image);
        assert_eq!(w.backdrop.as_deref(), Some("drawing-uuid"));
        let back: World = serde_yaml::from_str(&serde_yaml::to_string(&w).unwrap()).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn spatial_region_variants_round_trip() {
        for region in [
            SpatialRegion::SvgElement {
                element_id: "kitchen".into(),
            },
            SpatialRegion::BoundingBox {
                x: 0.1,
                y: 0.2,
                width: 0.3,
                height: 0.4,
            },
            SpatialRegion::Polygon {
                points: vec![(0.0, 0.0), (0.5, 0.1), (0.2, 0.9)],
            },
        ] {
            let back: SpatialRegion =
                serde_yaml::from_str(&serde_yaml::to_string(&region).unwrap()).unwrap();
            assert_eq!(region, back);
        }
    }
}
