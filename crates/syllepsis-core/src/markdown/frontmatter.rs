//! Reading and writing a note as one markdown file: YAML frontmatter (the [`Note`] fields)
//! delimited by `---`, followed by the body.
//!
//! The body is *not* part of the serde struct (it is `#[serde(skip)]` on [`Note`]); this
//! module is the single place that splits a file into frontmatter + body and stitches them
//! back together, so the rest of the crate works with a fully-formed [`Note`].

use crate::error::{CoreError, CoreResult};
use crate::model::Note;

/// Delimiter line that opens and closes YAML frontmatter.
const FENCE: &str = "---";

/// Serialize a note to its on-disk markdown representation (`---` frontmatter + body).
pub fn serialize_note(note: &Note) -> CoreResult<String> {
    let yaml = serde_yaml::to_string(note)?;
    let mut out = String::with_capacity(yaml.len() + note.body.len() + 16);
    out.push_str(FENCE);
    out.push('\n');
    out.push_str(&yaml);
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(FENCE);
    out.push('\n');
    if !note.body.is_empty() {
        out.push_str(&note.body);
        if !note.body.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// Parse a full markdown file into a [`Note`], attaching the body after the frontmatter.
pub fn parse_note(content: &str) -> CoreResult<Note> {
    let (frontmatter, body) = split_frontmatter(content).ok_or_else(|| {
        CoreError::parse("note file", "missing or unterminated `---` frontmatter")
    })?;
    let mut note: Note = serde_yaml::from_str(&frontmatter)?;
    // Single migration boundary: fan a legacy `private: true` flag out to the three capability
    // flags so every downstream feature (search, publish, views, sync) sees expanded state.
    note.metadata.lifecycle.normalize();
    note.body = body;
    Ok(note)
}

/// Split a file into `(frontmatter_yaml, body)`. Returns `None` if the file does not open
/// with a `---` fence or the fence is never closed. Line-based so it tolerates `\r\n`.
pub fn split_frontmatter(content: &str) -> Option<(String, String)> {
    // Strip a UTF-8 BOM that some external editors prepend.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.lines();
    if lines.next()?.trim() != FENCE {
        return None;
    }

    let mut frontmatter = String::new();
    let mut closed = false;
    let mut body_lines: Vec<&str> = Vec::new();
    for line in lines {
        if !closed && line.trim() == FENCE {
            closed = true;
            continue;
        }
        if closed {
            body_lines.push(line);
        } else {
            frontmatter.push_str(line);
            frontmatter.push('\n');
        }
    }
    if !closed {
        return None;
    }
    Some((frontmatter, body_lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ObjectType, PriorEdge};

    #[test]
    fn round_trips_a_note_with_body() {
        let mut note = Note::new(ObjectType::Note, "Energy basics", "syllepsis_001");
        note.summary = "Insulation first.".into();
        note.body = "Reduce loads before sizing equipment.\n\nThen optimize.".into();
        note.categories = vec!["energy".into()];
        note.prior = Some(PriorEdge::starts_category("energy"));

        let serialized = serialize_note(&note).unwrap();
        assert!(serialized.starts_with("---\n"));
        let parsed = parse_note(&serialized).unwrap();

        assert_eq!(parsed.id, note.id);
        assert_eq!(parsed.title, note.title);
        assert_eq!(parsed.summary, note.summary);
        assert_eq!(parsed.body, note.body);
        assert_eq!(parsed.categories, note.categories);
        assert_eq!(parsed.prior, note.prior);
    }

    #[test]
    fn rejects_file_without_frontmatter() {
        assert!(parse_note("just some text, no fence").is_err());
        assert!(split_frontmatter("no fence here").is_none());
    }

    #[test]
    fn tolerates_bom_and_crlf() {
        let mut note = Note::new(ObjectType::Note, "t", "syllepsis_001");
        note.body = "line".into();
        let serialized = serialize_note(&note).unwrap();
        let crlf = format!("\u{feff}{}", serialized.replace('\n', "\r\n"));
        let parsed = parse_note(&crlf).unwrap();
        assert_eq!(parsed.body, "line");
    }

    #[test]
    fn picture_asset_metadata_round_trips_in_frontmatter() {
        let mut note = Note::new(ObjectType::Picture, "Photo", "syllepsis_001");
        note.asset = Some(crate::model::AssetMetadata {
            uuid: "asset-1".into(),
            media_type: "image/png".into(),
            intrinsic_dimensions: (640, 480),
            original_filename: "photo.png".into(),
        });
        let parsed = parse_note(&serialize_note(&note).unwrap()).unwrap();
        assert_eq!(parsed.asset, note.asset);
    }
}
