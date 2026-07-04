//! File-based persistence: book folder layout, the note store seam, the id registry, and the
//! `Book` handle that ties them together.

pub mod atomic;
pub mod book;
pub mod layout;
pub mod registry;
pub mod store;

pub use atomic::write_atomic;
pub use book::{Book, BookMetadata};
pub use registry::IdRegistry;
pub use store::{FsNoteStore, NoteLoadIssue, NoteStore};
