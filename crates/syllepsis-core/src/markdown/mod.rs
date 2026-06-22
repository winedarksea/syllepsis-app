//! Markdown handling: frontmatter (de)serialization, the Syllepsis inline dialect, and a
//! pulldown-cmark parser wrapper.

pub mod dialect;
pub mod frontmatter;
pub mod parser;

pub use dialect::Cloze;
pub use frontmatter::{parse_note, serialize_note, split_frontmatter};
pub use parser::{section_anchors, to_html, SectionAnchor};
