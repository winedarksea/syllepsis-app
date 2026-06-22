//! Application layer: the framework-agnostic command surface and its DTOs. The Tauri shell
//! wraps these as commands; a PWA worker can call them directly.

pub mod commands;
pub mod dto;
pub mod llm;
pub mod search;
pub mod spatial;
pub mod sync;

pub use dto::NoteDto;
