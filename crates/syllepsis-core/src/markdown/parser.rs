//! Thin wrapper over `pulldown-cmark`. Kept behind this seam so the parser can be swapped
//! without touching callers (AGENTS.md: clean library boundaries).
//!
//! Two jobs in Phase 1:
//! - [`section_anchors`] enumerates a body's headings so links of the form
//!   `note-id#section-heading` (ui-views.md) can resolve to a position inside a note.
//! - [`to_html`] renders a body to HTML (comments stripped first) for previews and the
//!   future read-only server view.

use crate::id::slugify;
use crate::markdown::dialect;
use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// A markdown heading within a note body — a link target for intra-note section links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionAnchor {
    /// Heading depth 1–6.
    pub level: u8,
    /// The heading's text content.
    pub text: String,
    /// A kebab-case anchor slug derived from the text (the `#section-heading` fragment).
    pub slug: String,
}

/// Enumerate the headings in a body, in document order.
pub fn section_anchors(body: &str) -> Vec<SectionAnchor> {
    let mut anchors = Vec::new();
    let mut current: Option<(u8, String)> = None;

    for event in Parser::new_ext(body, Options::empty()) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((heading_level_to_u8(level), String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, ref mut buffer)) = current {
                    buffer.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = current.take() {
                    let slug = slugify(&text);
                    anchors.push(SectionAnchor { level, text, slug });
                }
            }
            _ => {}
        }
    }
    anchors
}

/// Render a body to HTML, stripping `%%comments%%` first. Standard CommonMark plus tables and
/// strikethrough; the Syllepsis-specific inline tokens (cloze, loc) are rendered by the
/// frontend editor, not here.
pub fn to_html(body: &str) -> String {
    let cleaned = dialect::strip_comments(body);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&cleaned, options);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_section_anchors() {
        let body = "# Overview\n\nText.\n\n## Energy Loads\n\nMore.";
        let anchors = section_anchors(body);
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0].level, 1);
        assert_eq!(anchors[0].slug, "overview");
        assert_eq!(anchors[1].level, 2);
        assert_eq!(anchors[1].slug, "energy-loads");
    }

    #[test]
    fn renders_html_without_comments() {
        let html = to_html("Hello %%internal note%% **world**");
        assert!(html.contains("<strong>world</strong>"));
        assert!(!html.contains("internal note"));
    }
}
