//! Knowledge packs (core-concepts.md "Knowledge Packs"): curated, versioned collections of notes
//! that load **into** an existing book rather than standing alone.
//!
//! This module is the portable, book-agnostic core: the [`Pack`] envelope (manifest + notes +
//! the categories those notes use) and its on-disk (de)serialization. A pack is a single
//! self-contained JSON file so it is trivially distributable — no archive format, no loose
//! directory to keep together. The book-aware export/import flow (selection, category mapping,
//! the `locally_modified` overwrite protection) lives in [`crate::app::pack`].

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::model::{Category, Note, ObjectType};

/// Envelope format tag written into every pack file, so a reader can reject an incompatible
/// future format instead of silently mis-parsing it.
pub const PACK_FORMAT: &str = "syllepsis_pack_001";

/// Whether a pack was exported as a full book archive or a curated subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    #[default]
    Pack,
    Book,
}

/// Identity and descriptive metadata for a pack (core-concepts.md: "Knowledge packs carry a
/// version number"). `id` is the stable key used for `PackMembership` and version re-imports;
/// `version` is the user-facing pack version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub export_kind: ExportKind,
}

/// One note's portable content inside a pack. Only the shareable fields travel — none of the
/// device-local derived state (vectors, sync sidecars) and none of the privacy/lifecycle flags,
/// which belong to the receiving book, not the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackNote {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: ObjectType,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

impl PackNote {
    /// Project a stored note into its portable pack form (dropping local-only state).
    pub fn from_note(note: &Note) -> PackNote {
        PackNote {
            id: note.id.to_string(),
            object_type: note.object_type,
            title: note.title.clone(),
            summary: note.summary.clone(),
            body: note.body.clone(),
            categories: note.categories.clone(),
        }
    }
}

/// A complete, distributable knowledge pack. (No `Eq`: bundled [`Category`] definitions may carry
/// `f64` spatial coordinates, which are not `Eq`.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    /// Envelope format tag (see [`PACK_FORMAT`]).
    pub format: String,
    pub manifest: PackManifest,
    pub notes: Vec<PackNote>,
    /// Definitions for the categories the notes reference, so the import side can recreate them
    /// (icon, heading level, …) instead of guessing from a bare name.
    #[serde(default)]
    pub categories: Vec<Category>,
}

impl Pack {
    /// Assemble a pack from already-selected notes and category definitions.
    pub fn new(manifest: PackManifest, notes: Vec<PackNote>, categories: Vec<Category>) -> Pack {
        Pack {
            format: PACK_FORMAT.to_string(),
            manifest,
            notes,
            categories,
        }
    }

    /// Serialize to pretty JSON (human-diffable, since packs are shared and version-controlled).
    pub fn to_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse a pack from JSON, rejecting an unrecognized envelope format.
    pub fn from_json(json: &str) -> CoreResult<Pack> {
        let pack: Pack = serde_json::from_str(json)?;
        if pack.format != PACK_FORMAT {
            return Err(CoreError::parse(
                "pack",
                format!(
                    "unsupported pack format '{}' (expected '{PACK_FORMAT}')",
                    pack.format
                ),
            ));
        }
        Ok(pack)
    }

    /// Write the pack to a file path (the distributable artifact).
    pub fn write_to(&self, path: &Path) -> CoreResult<()> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Read a pack from a file path.
    pub fn read_from(path: &Path) -> CoreResult<Pack> {
        Pack::from_json(&std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Pack {
        Pack::new(
            PackManifest {
                id: "permaculture-basics".into(),
                name: "Permaculture Basics".into(),
                version: "1.2.0".into(),
                description: "Starter notes on permaculture.".into(),
                export_kind: ExportKind::Pack,
            },
            vec![PackNote {
                id: "note-compost-01HABC".into(),
                object_type: ObjectType::Note,
                title: "Compost".into(),
                summary: "How to compost".into(),
                body: "Greens and browns.".into(),
                categories: vec!["garden".into()],
            }],
            vec![Category::new("garden")],
        )
    }

    #[test]
    fn pack_json_round_trips() {
        let pack = sample();
        let back = Pack::from_json(&pack.to_json().unwrap()).unwrap();
        assert_eq!(pack, back);
        assert_eq!(back.format, PACK_FORMAT);
    }

    #[test]
    fn file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("basics.synpack.json");
        let pack = sample();
        pack.write_to(&path).unwrap();
        assert_eq!(Pack::read_from(&path).unwrap(), pack);
    }

    #[test]
    fn rejects_an_unknown_format() {
        let json = r#"{"format":"from_the_future","manifest":{"id":"x","name":"x","version":"1"},"notes":[],"categories":[]}"#;
        assert!(Pack::from_json(json).is_err());
    }

    #[test]
    fn export_kind_defaults_to_pack_for_legacy_json() {
        let json = r#"{"format":"syllepsis_pack_001","manifest":{"id":"x","name":"x","version":"1"},"notes":[],"categories":[]}"#;
        let pack = Pack::from_json(json).unwrap();
        assert_eq!(pack.manifest.export_kind, ExportKind::Pack);
    }
}
