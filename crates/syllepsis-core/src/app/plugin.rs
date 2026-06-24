//! Application command surface for plugins: the framework-agnostic operations the Tauri shell (and
//! a future PWA worker) wrap. Discovery is always available; the run paths need the WASM host and
//! so are compiled behind the `extism` feature, like the host itself.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::plugin::{PluginKind, PluginRegistry, PluginSource};

/// A plugin as presented to the UI (Settings list, editor language map). Flattens the manifest
/// plus where it came from; deliberately omits the on-disk wasm path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: PluginKind,
    pub languages: Vec<String>,
    pub import_extensions: Vec<String>,
    pub source: PluginSource,
    pub enabled: bool,
}

/// List every installed plugin for the UI. Disabled plugins are included but marked `enabled:
/// false`; the caller decides whether to surface them.
pub fn list_plugins(
    registry: &PluginRegistry,
    disabled: &HashSet<String>,
) -> Vec<PluginDescriptor> {
    registry
        .all()
        .iter()
        .map(|installed| PluginDescriptor {
            id: installed.manifest.id.clone(),
            name: installed.manifest.name.clone(),
            version: installed.manifest.version.clone(),
            description: installed.manifest.description.clone(),
            kind: installed.manifest.kind,
            languages: installed.manifest.languages.clone(),
            import_extensions: installed.manifest.import_extensions.clone(),
            source: installed.source,
            enabled: !disabled.contains(&installed.manifest.id),
        })
        .collect()
}

#[cfg(feature = "extism")]
pub use run::{import_via_plugin, run_render_plugin};

#[cfg(feature = "extism")]
mod run {
    use super::*;
    use crate::app::text_import::{self, TextImportOptions, TextImportPreview};
    use crate::error::{CoreError, CoreResult};
    use crate::plugin::PluginHost;

    /// Run an import-source plugin over raw file bytes and produce the same review-then-commit
    /// preview the text importer uses, so the rest of the import flow is unchanged.
    pub fn import_via_plugin(
        host: &PluginHost,
        registry: &PluginRegistry,
        disabled: &HashSet<String>,
        plugin_id: &str,
        bytes: &[u8],
        options: &TextImportOptions,
    ) -> CoreResult<TextImportPreview> {
        if disabled.contains(plugin_id) {
            return Err(CoreError::Plugin(format!(
                "plugin '{plugin_id}' is disabled"
            )));
        }
        let plugin = registry
            .get(plugin_id)
            .ok_or_else(|| CoreError::Plugin(format!("unknown plugin '{plugin_id}'")))?;
        if plugin.manifest.kind != PluginKind::ImportSource {
            return Err(CoreError::Plugin(format!(
                "plugin '{plugin_id}' is not an import source"
            )));
        }
        let text = host.run_import(plugin_id, bytes)?;
        Ok(text_import::preview_text_import(&text, options))
    }

    /// Render a fenced code block with the first *enabled* plugin that claims its language;
    /// returns the plugin's (unsanitized) HTML. The caller sanitizes before display.
    pub fn run_render_plugin(
        host: &PluginHost,
        registry: &PluginRegistry,
        disabled: &HashSet<String>,
        language: &str,
        code: &str,
    ) -> CoreResult<String> {
        let plugin = registry
            .render_plugin_for_language(language)
            .ok_or_else(|| CoreError::Plugin(format!("no renderer for language '{language}'")))?;
        if disabled.contains(&plugin.manifest.id) {
            return Err(CoreError::Plugin(format!(
                "no renderer for language '{language}'"
            )));
        }
        host.run_render(&plugin.manifest.id, language, code)
    }
}
