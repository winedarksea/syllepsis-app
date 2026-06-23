//! Sandboxed (WASM) plugins: the extension seam that lets new import sources and code-block
//! renderers plug in without changing the core.
//!
//! The shape mirrors the rest of the crate's optional-capability modules ([`crate::crdt`],
//! [`crate::onnx`]): the declarative, always-compiled half — the [`manifest`] and the [`registry`]
//! that discovers installed plugins — has no WASM dependency, so every build can *list* what is
//! installed. The runtime that actually executes a plugin ([`host::PluginHost`]) lives behind the
//! `extism` feature.
//!
//! Two hook surfaces are defined this milestone, one per [`manifest::PluginKind`]:
//! - **import source** — `import(bytes) -> { text }`, feeding the existing text-import pipeline;
//! - **code-block renderer** — `render(lang, code) -> { html }`, rendered (sanitized) in the editor.
//!
//! Plugins change notes only through the small note-write host API ([`note_write`]), which routes
//! every change through the normal [`crate::app::commands`] paths so it flows into the CRDT sidecar
//! at sync time like any other edit.

pub mod manifest;
pub mod note_write;
pub mod registry;

#[cfg(feature = "extism")]
pub mod host;

pub use manifest::{PluginKind, PluginManifest, PLUGIN_MANIFEST_FORMAT};
pub use note_write::CreateNoteInput;
pub use registry::{InstalledPlugin, PluginRegistry, PluginSource};

#[cfg(feature = "extism")]
pub use host::PluginHost;
