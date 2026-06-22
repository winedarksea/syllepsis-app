//! The object-type enum. Every note is one of these; the variant's [`ObjectType::id_prefix`]
//! becomes the human-readable `{type}` portion of a [`crate::NoteId`].
//!
//! Text-ish types share the summary/description model in [`super::note`]; the special
//! non-text types (table, picture, drawing, code) carry their payload in companion files
//! (CSV, raster, SVG) with frontmatter alongside — see object-types.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// Default text note.
    #[default]
    Note,
    /// Text plus a reference — content written by someone else.
    Quote,
    /// Bibliographic entry (author/year/title/url); tagged with `@`.
    Reference,
    /// Checklist-only text note with todo syntax sugar.
    Todo,
    /// Question/answer: renames summary→question, description→answer, shows both.
    Qa,
    /// AI proposals, fact checks, and rewrites linked by id to another note.
    Commentary,
    /// Tabular data stored as CSV (subtypes: decision matrix, pro/con).
    Table,
    /// Raster image; caption/metadata in XMP. Not CRDT-tracked.
    Picture,
    /// Vector drawing stored as SVG (imported SVGs are drawings too).
    Drawing,
    /// Code block; Mermaid is a render-inline subtype.
    Code,
}

impl ObjectType {
    /// The lowercase prefix used in ids and filenames.
    pub fn id_prefix(self) -> &'static str {
        match self {
            ObjectType::Note => "note",
            ObjectType::Quote => "quote",
            ObjectType::Reference => "reference",
            ObjectType::Todo => "todo",
            ObjectType::Qa => "qa",
            ObjectType::Commentary => "commentary",
            ObjectType::Table => "table",
            ObjectType::Picture => "picture",
            ObjectType::Drawing => "drawing",
            ObjectType::Code => "code",
        }
    }

    /// Inverse of [`ObjectType::id_prefix`]; used when interpreting an id's type segment.
    pub fn from_id_prefix(prefix: &str) -> Option<ObjectType> {
        let ty = match prefix {
            "note" => ObjectType::Note,
            "quote" => ObjectType::Quote,
            "reference" => ObjectType::Reference,
            "todo" => ObjectType::Todo,
            "qa" => ObjectType::Qa,
            "commentary" => ObjectType::Commentary,
            "table" => ObjectType::Table,
            "picture" => ObjectType::Picture,
            "drawing" => ObjectType::Drawing,
            "code" => ObjectType::Code,
            _ => return None,
        };
        Some(ty)
    }

    /// True for types whose payload is the markdown body itself (vs. a companion file).
    pub fn is_text(self) -> bool {
        matches!(
            self,
            ObjectType::Note
                | ObjectType::Quote
                | ObjectType::Reference
                | ObjectType::Todo
                | ObjectType::Qa
                | ObjectType::Commentary
                | ObjectType::Code
        )
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
            ObjectType::Quote,
            ObjectType::Reference,
            ObjectType::Todo,
            ObjectType::Qa,
            ObjectType::Commentary,
            ObjectType::Table,
            ObjectType::Picture,
            ObjectType::Drawing,
            ObjectType::Code,
        ] {
            assert_eq!(ObjectType::from_id_prefix(ty.id_prefix()), Some(ty));
        }
        assert_eq!(ObjectType::from_id_prefix("bogus"), None);
    }
}
