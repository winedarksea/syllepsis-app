//! Spatial worlds & overlays (spatial-worlds.md): placing notes and categories in a coordinate
//! space — a real map, a floorplan, or a memory palace — not just in the narrative tree.
//!
//! The [`World`](crate::model::World) registry types live in [`crate::model`] (they are on-disk
//! objects); this module is the **behavior** layered on top:
//! - [`location`] — parsing the `loc:` grammar into an unresolved coordinate.
//! - [`registry`] — the in-memory set of worlds (always including `earth`).
//! - [`lookup`] — the CSV text→coordinate table that resolves named places, carrying a `world`
//!   column so a plain string resolves within its world.
//! - [`resolve`] — turning a parsed token + the registry + the lookup into a concrete coordinate.
//! - [`overlay`] — assembling a world's pins (notes/categories as points) and regions (clickable
//!   category areas) for rendering.
//!
//! First-pass scope is image-backed worlds (floorplans, mind palaces) and their overlays; geo map
//! tiles are a later extension (spatial-worlds.md "Map view (future)").

pub mod location;
pub mod lookup;
pub mod overlay;
pub mod projection;
pub mod registry;
pub mod resolve;

pub use location::{parse_location, LocationValue, ParsedLocation, ResolvedLocation, WorldPoint};
pub use lookup::{LocationLookup, LookupEntry};
pub use overlay::{build_overlay, Overlay, OverlayRegion, Pin, SpatialTarget};
pub use projection::{equal_earth_forward, equal_earth_inverse, equal_earth_normalized};
pub use registry::WorldRegistry;
pub use resolve::{resolve, resolve_token};
