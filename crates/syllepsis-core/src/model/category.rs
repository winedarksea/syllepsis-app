//! Categories serve two purposes (core-concepts.md): linking notes to a topic (as a
//! hashtag) and acting as chapters/sections in book view.
//!
//! Categories live in the book's `_categories/` folder as small frontmatter files. They form
//! their own hierarchy via [`Category::parent`]; **a category's parent is always another
//! category, never a note**.

use serde::{Deserialize, Serialize};

use crate::model::world::SpatialRegion;

/// Default visual heading weight when a category does not specify one.
const DEFAULT_HEADING_LEVEL: u8 = 2;

// No `Eq`: a `region` may carry `f64` normalized coordinates (`SpatialRegion`), which are not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    /// No-whitespace name used as a hashtag (e.g. `electrical` → `#electrical`). Canonical key.
    pub name: String,
    /// Display heading (may contain whitespace), e.g. "Electrical Systems".
    #[serde(default)]
    pub long_name: String,
    /// Visual heading weight H1–H6+. **Stylistic only** — it does not set hierarchy position,
    /// except as a tiebreaker between siblings sharing a parent (see `sort`).
    #[serde(default = "default_heading_level")]
    pub heading_level: u8,
    /// Optional icon (like a book cover) for visual distinction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Parent category name; `None` for a top-level category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Optional spatial location token (e.g. the `#kitchen` pin on a floorplan). Stored as the
    /// raw `loc:` token; its world prefix anchors the category in a [world](SpatialRegion) and the
    /// coordinate is its pin/anchor. Resolution is in [`crate::spatial`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Optional clickable **area** in the same world as [`Self::location`]. When set, the category
    /// renders as a region (a room on a floorplan) rather than a single pin; clicking it runs the
    /// filtered-sorted view for this category. `location` still supplies the world + anchor point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<SpatialRegion>,
    /// Excluded from the GitHub publish and from RAG/default views when private.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
}

fn default_heading_level() -> u8 {
    DEFAULT_HEADING_LEVEL
}

impl Category {
    /// Create a category from a hashtag name, defaulting the display name to the same value.
    pub fn new(name: impl Into<String>) -> Category {
        let name = name.into();
        Category {
            long_name: name.clone(),
            name,
            heading_level: DEFAULT_HEADING_LEVEL,
            icon: None,
            parent: None,
            location: None,
            region: None,
            private: false,
        }
    }

    /// The heading text shown in book view (falls back to the hashtag name if unset).
    pub fn heading_text(&self) -> &str {
        if self.long_name.is_empty() {
            &self.name
        } else {
            &self.long_name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_heading_fallback() {
        let mut c = Category::new("electrical");
        assert_eq!(c.heading_level, DEFAULT_HEADING_LEVEL);
        assert_eq!(c.heading_text(), "electrical");
        c.long_name = "Electrical Systems".into();
        assert_eq!(c.heading_text(), "Electrical Systems");
    }

    #[test]
    fn round_trips() {
        let c = Category::new("kitchen");
        let yaml = serde_yaml::to_string(&c).unwrap();
        let back: Category = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(c, back);
    }
}
