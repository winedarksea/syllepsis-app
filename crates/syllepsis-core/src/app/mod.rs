//! Application layer: the framework-agnostic command surface and its DTOs. The Tauri shell
//! wraps these as commands; a PWA worker can call them directly.

pub mod commands;
pub mod dto;
pub mod lifecycle;
pub mod llm;
pub mod pack;
pub mod publish;
pub mod search;
pub mod spatial;
pub mod sync;
pub mod text_import;

pub use dto::NoteDto;
