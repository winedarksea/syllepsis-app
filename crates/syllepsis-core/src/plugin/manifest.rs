//! The plugin manifest: the small, declarative descriptor every plugin ships alongside its
//! `.wasm` module. It is the plugin analogue of [`crate::pack::PackManifest`] — identity plus the
//! capability metadata the host needs to decide *where* a plugin plugs in (which fenced-code
//! language it renders, which file extensions it imports) before it ever loads the WASM.

use serde::{Deserialize, Serialize};

/// Envelope tag written into every manifest so a reader can reject an incompatible future format
/// instead of silently mis-parsing it (mirrors [`crate::pack::PACK_FORMAT`]).
pub const PLUGIN_MANIFEST_FORMAT: &str = "syllepsis_plugin_001";

/// Conventional manifest filename inside a plugin directory.
pub const MANIFEST_FILE_NAME: &str = "plugin.json";

/// The hook surface a plugin plugs into. This is what decides how the host invokes it; the two
/// initial kinds map to the two host entry points (`run_import` / `run_render`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// A data-ingestion source: `import(bytes) -> { text }`. The returned text is fed into the
    /// existing text-import preview→chunk→commit pipeline.
    ImportSource,
    /// A fenced-code-block renderer: `render(lang, code) -> { html }`. The returned HTML is
    /// sanitized in the webview and rendered in place of the raw code block.
    CodeBlockRenderer,
}

/// A plugin's declarative descriptor. Deserialized from `plugin.json`; serialized (via the app
/// DTO) to the frontend so the Settings panel and editor can discover what is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Envelope format tag (see [`PLUGIN_MANIFEST_FORMAT`]).
    pub format: String,
    /// Stable unique id (e.g. `"pdf-import"`). Used as the host's plugin key and in config.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub kind: PluginKind,
    /// The `.wasm` filename, relative to the manifest's directory.
    pub entry_wasm: String,
    /// For [`PluginKind::CodeBlockRenderer`]: the fenced-code language tags this plugin renders
    /// (e.g. `["rust", "python"]`). Ignored for other kinds.
    #[serde(default)]
    pub languages: Vec<String>,
    /// For [`PluginKind::ImportSource`]: the lower-case file extensions this plugin imports
    /// (e.g. `["pdf"]`), used to label the source and filter the file dialog. Ignored otherwise.
    #[serde(default)]
    pub import_extensions: Vec<String>,
    /// Free-form capability tags reserved for future host-permission gating (e.g. `"note_write"`).
    /// Recorded but not yet enforced this milestone.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginManifest {
    /// Parse a manifest from its JSON bytes, rejecting an unknown envelope format.
    pub fn from_json(bytes: &[u8]) -> crate::error::CoreResult<PluginManifest> {
        let manifest: PluginManifest = serde_json::from_slice(bytes)?;
        if manifest.format != PLUGIN_MANIFEST_FORMAT {
            return Err(crate::error::CoreError::Plugin(format!(
                "unsupported plugin manifest format '{}' (expected '{}')",
                manifest.format, PLUGIN_MANIFEST_FORMAT
            )));
        }
        if manifest.id.trim().is_empty() {
            return Err(crate::error::CoreError::Plugin(
                "plugin manifest is missing an id".to_string(),
            ));
        }
        if manifest.entry_wasm.trim().is_empty() {
            return Err(crate::error::CoreError::Plugin(format!(
                "plugin '{}' manifest is missing entry_wasm",
                manifest.id
            )));
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = PluginManifest {
            format: PLUGIN_MANIFEST_FORMAT.to_string(),
            id: "syntax-highlight".to_string(),
            name: "Syntax Highlighter".to_string(),
            version: "0.1.0".to_string(),
            description: "Highlights fenced code blocks.".to_string(),
            kind: PluginKind::CodeBlockRenderer,
            entry_wasm: "syntax_highlight.wasm".to_string(),
            languages: vec!["rust".to_string(), "toml".to_string()],
            import_extensions: vec![],
            capabilities: vec![],
        };
        let json = serde_json::to_vec(&manifest).unwrap();
        let parsed = PluginManifest::from_json(&json).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn rejects_unknown_format() {
        let json = br#"{"format":"nope","id":"x","name":"X","version":"1","kind":"import_source","entry_wasm":"x.wasm"}"#;
        assert!(matches!(
            PluginManifest::from_json(json).unwrap_err(),
            crate::error::CoreError::Plugin(_)
        ));
    }
}
