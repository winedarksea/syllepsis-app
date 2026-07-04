//! Sorting: build the prior-relationship tree and flatten it into book view.

pub mod book_render;
pub mod tree;

pub use book_render::{
    flatten, render, render_split, to_markdown, to_markdown_anchored, ChapterRender, RenderItem,
    RenderedNote, SplitRender,
};
pub use tree::{build, CategoryNode, NoteNode, SortTree};
