//! The object-type enum. This describes storage shape only; note subtypes such as Q&A, quotes,
//! references, code, and todos live in [`super::classification::ClassificationKind`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// Markdown-backed text note.
    #[default]
    Note,
    /// AI proposals, fact checks, and rewrites linked by id to another note.
    Commentary,
    /// Tabular data stored as CSV.
    Table,
    /// Raster image; caption/metadata in XMP. Not CRDT-tracked.
    Picture,
    /// Vector drawing stored as SVG (imported SVGs are drawings too).
    Drawing,
}

impl ObjectType {
    /// The lowercase prefix used in ids and filenames.
    pub fn id_prefix(self) -> &'static str {
        match self {
            ObjectType::Note => "note",
            ObjectType::Commentary => "commentary",
            ObjectType::Table => "table",
            ObjectType::Picture => "picture",
            ObjectType::Drawing => "drawing",
        }
    }

    /// Inverse of [`ObjectType::id_prefix`]; used when interpreting an id's type segment.
    pub fn from_id_prefix(prefix: &str) -> Option<ObjectType> {
        let ty = match prefix {
            "note" => ObjectType::Note,
            "commentary" => ObjectType::Commentary,
            "table" => ObjectType::Table,
            "picture" => ObjectType::Picture,
            "drawing" => ObjectType::Drawing,
            _ => return None,
        };
        Some(ty)
    }

    /// True for types whose payload is the markdown body itself (vs. a companion file).
    pub fn is_text(self) -> bool {
        matches!(self, ObjectType::Note | ObjectType::Commentary)
    }
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id_prefix())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_round_trips() {
        for ty in [
            ObjectType::Note,
            ObjectType::Commentary,
            ObjectType::Table,
            ObjectType::Picture,
            ObjectType::Drawing,
        ] {
            assert_eq!(ObjectType::from_id_prefix(ty.id_prefix()), Some(ty));
        }
        assert_eq!(ObjectType::from_id_prefix("bogus"), None);
    }
}
