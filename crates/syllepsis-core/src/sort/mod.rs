//! Sorting: build the prior-relationship tree and flatten it into book view.

pub mod book_render;
pub mod tree;

pub use book_render::{flatten, render, to_markdown, RenderItem, RenderedNote};
pub use tree::{build, CategoryNode, NoteNode, SortTree};
