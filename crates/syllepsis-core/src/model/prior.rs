//! The sorting primitive. Sorting is a tree built from **prior** relationships
//! (core-concepts.md): every sorted note points at the note *or category* it follows, and
//! the [`PriorKind`] determines how the two render together in book view.

use crate::id::NoteId;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How a note joins onto its prior in the rendered book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriorKind {
    /// Standard paragraph break between the two notes (default).
    #[default]
    NewParagraph,
    /// Flow into the same paragraph, space-separated.
    SameParagraph,
    /// New paragraph indented one level (not recursive).
    IndentedNewParagraph,
    /// Group with siblings as a bulleted list; a bullet under a bullet nests.
    BulletPoint,
    /// Same as bullet but numbered.
    NumberedList,
}

impl PriorKind {
    /// Bullet/numbered items sharing a prior are grouped into one list — the exception to the
    /// rule that multiple children of the same prior create branching.
    pub fn is_list_item(self) -> bool {
        matches!(self, PriorKind::BulletPoint | PriorKind::NumberedList)
    }
}

/// What a note follows: another note, or a category (making it the first note of a section).
///
/// Serialized as a clean single-key map — `note: <id>` or `category: <name>` — via manual
/// impls. (serde's externally-tagged derive would render YAML tags like `!category`, which
/// is ugly in frontmatter read outside the app.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorRef {
    /// Follows another note directly.
    Note(NoteId),
    /// Is the first note of this category's section.
    Category(String),
}

impl Serialize for PriorRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            PriorRef::Note(id) => map.serialize_entry("note", id)?,
            PriorRef::Category(name) => map.serialize_entry("category", name)?,
        }
        map.end()
    }
}

/// Mirror struct used only for deserialization, then validated to exactly one variant.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PriorRefMirror {
    #[serde(default)]
    note: Option<NoteId>,
    #[serde(default)]
    category: Option<String>,
}

impl<'de> Deserialize<'de> for PriorRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mirror = PriorRefMirror::deserialize(deserializer)?;
        match (mirror.note, mirror.category) {
            (Some(note), None) => Ok(PriorRef::Note(note)),
            (None, Some(category)) => Ok(PriorRef::Category(category)),
            _ => Err(D::Error::custom(
                "prior `target` must have exactly one of `note` or `category`",
            )),
        }
    }
}

/// A complete prior relationship: what the note follows and how it joins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorEdge {
    pub target: PriorRef,
    #[serde(default)]
    pub kind: PriorKind,
}

impl PriorEdge {
    /// A note that starts a category section with a plain paragraph.
    pub fn starts_category(category: impl Into<String>) -> Self {
        PriorEdge {
            target: PriorRef::Category(category.into()),
            kind: PriorKind::NewParagraph,
        }
    }

    /// A note following another note with the given join kind.
    pub fn follows(note: NoteId, kind: PriorKind) -> Self {
        PriorEdge {
            target: PriorRef::Note(note),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_serializes_cleanly() {
        let edge = PriorEdge::starts_category("electrical");
        let yaml = serde_yaml::to_string(&edge).unwrap();
        assert!(yaml.contains("category: electrical"));
        assert!(yaml.contains("kind: new_paragraph"));
        let back: PriorEdge = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(edge, back);
    }

    #[test]
    fn list_item_detection() {
        assert!(PriorKind::BulletPoint.is_list_item());
        assert!(PriorKind::NumberedList.is_list_item());
        assert!(!PriorKind::NewParagraph.is_list_item());
    }
}
