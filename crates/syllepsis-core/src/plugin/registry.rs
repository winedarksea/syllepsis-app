//! Plugin discovery: scan one or more directories for plugin manifests and present the validated
//! set. This is deliberately free of any WASM dependency — it only reads JSON and resolves file
//! paths, so it compiles in every build (the always-on half of the seam, like
//! [`crate::crdt`]'s LWW backend). Actually *running* a plugin lives behind the `extism` feature in
//! [`super::host`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::{PluginManifest, MANIFEST_FILE_NAME};

/// Where an installed plugin came from. Surfaced in the UI so the user can tell a bundled plugin
/// from one they dropped in themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    /// Shipped with the app (bundled as a Tauri resource).
    Builtin,
    /// Discovered in the user's plugin directory.
    User,
}

/// A discovered, manifest-valid plugin with its resolved on-disk `.wasm` path.
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    /// Absolute path to the entry `.wasm`, resolved against the manifest's directory.
    pub wasm_path: PathBuf,
}

/// The set of plugins found at startup. Built once (discovery touches the filesystem); lookups are
/// in-memory and cheap.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    plugins: Vec<InstalledPlugin>,
}

impl PluginRegistry {
    /// Discover plugins from the bundled directory and, if present, the user's plugin directory.
    /// User plugins win on an id collision (so a user can shadow a built-in). Malformed plugins are
    /// logged and skipped rather than failing the whole scan.
    pub fn discover(builtin_dir: Option<&Path>, user_dir: Option<&Path>) -> PluginRegistry {
        let mut plugins: Vec<InstalledPlugin> = Vec::new();
        for plugin in builtin_dir.into_iter().flat_map(|d| scan_dir(d, PluginSource::Builtin)) {
            plugins.push(plugin);
        }
        for plugin in user_dir.into_iter().flat_map(|d| scan_dir(d, PluginSource::User)) {
            // A user plugin replaces a built-in with the same id.
            plugins.retain(|p| p.manifest.id != plugin.manifest.id);
            plugins.push(plugin);
        }
        PluginRegistry { plugins }
    }

    /// All installed plugins.
    pub fn all(&self) -> &[InstalledPlugin] {
        &self.plugins
    }

    /// Look a plugin up by its manifest id.
    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.iter().find(|p| p.manifest.id == id)
    }

    /// The first code-block renderer that claims the given fenced-code language tag.
    pub fn render_plugin_for_language(&self, language: &str) -> Option<&InstalledPlugin> {
        use super::manifest::PluginKind;
        let language = language.to_lowercase();
        self.plugins.iter().find(|p| {
            p.manifest.kind == PluginKind::CodeBlockRenderer
                && p.manifest
                    .languages
                    .iter()
                    .any(|l| l.to_lowercase() == language)
        })
    }
}

/// Scan a single directory for `*/plugin.json` manifests. Each plugin lives in its own
/// subdirectory holding the manifest and its `.wasm`.
fn scan_dir(dir: &Path, source: PluginSource) -> Vec<InstalledPlugin> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return found, // a missing user dir is normal, not an error.
    };
    for entry in entries.flatten() {
        let manifest_path = entry.path().join(MANIFEST_FILE_NAME);
        if !manifest_path.is_file() {
            continue;
        }
        match load_plugin(&manifest_path, source) {
            Ok(plugin) => found.push(plugin),
            Err(error) => {
                tracing::warn!(path = %manifest_path.display(), %error, "skipping invalid plugin");
            }
        }
    }
    found
}

fn load_plugin(
    manifest_path: &Path,
    source: PluginSource,
) -> crate::error::CoreResult<InstalledPlugin> {
    let bytes = std::fs::read(manifest_path)?;
    let manifest = PluginManifest::from_json(&bytes)?;
    let dir = manifest_path.parent().unwrap_or(Path::new("."));
    let wasm_path = dir.join(&manifest.entry_wasm);
    if !wasm_path.is_file() {
        return Err(crate::error::CoreError::Plugin(format!(
            "plugin '{}' entry_wasm not found at {}",
            manifest.id,
            wasm_path.display()
        )));
    }
    Ok(InstalledPlugin {
        manifest,
        source,
        wasm_path,
    })
}

#[cfg(test)]
mod tests {
    use super::super::manifest::{PluginKind, PLUGIN_MANIFEST_FORMAT};
    use super::*;
    use tempfile::tempdir;

    fn write_plugin(root: &Path, id: &str, kind: &str, langs: &[&str]) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("entry.wasm"), b"\0asm").unwrap();
        let langs_json = langs
            .iter()
            .map(|l| format!("\"{l}\""))
            .collect::<Vec<_>>()
            .join(",");
        std::fs::write(
            dir.join(MANIFEST_FILE_NAME),
            format!(
                r#"{{"format":"{PLUGIN_MANIFEST_FORMAT}","id":"{id}","name":"{id}","version":"1","kind":"{kind}","entry_wasm":"entry.wasm","languages":[{langs_json}]}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn discovers_builtin_plugins_and_resolves_wasm_path() {
        let dir = tempdir().unwrap();
        write_plugin(dir.path(), "syntax-highlight", "code_block_renderer", &["rust"]);
        let registry = PluginRegistry::discover(Some(dir.path()), None);
        assert_eq!(registry.all().len(), 1);
        let plugin = registry.get("syntax-highlight").unwrap();
        assert_eq!(plugin.source, PluginSource::Builtin);
        assert_eq!(plugin.manifest.kind, PluginKind::CodeBlockRenderer);
        assert!(plugin.wasm_path.is_file());
        assert!(registry.render_plugin_for_language("RUST").is_some());
        assert!(registry.render_plugin_for_language("python").is_none());
    }

    #[test]
    fn user_plugin_shadows_builtin_with_same_id() {
        let builtin = tempdir().unwrap();
        let user = tempdir().unwrap();
        write_plugin(builtin.path(), "shared", "import_source", &[]);
        write_plugin(user.path(), "shared", "import_source", &[]);
        let registry = PluginRegistry::discover(Some(builtin.path()), Some(user.path()));
        assert_eq!(registry.all().len(), 1);
        assert_eq!(registry.get("shared").unwrap().source, PluginSource::User);
    }

    #[test]
    fn skips_plugin_with_missing_wasm() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("broken");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join(MANIFEST_FILE_NAME),
            format!(
                r#"{{"format":"{PLUGIN_MANIFEST_FORMAT}","id":"broken","name":"broken","version":"1","kind":"import_source","entry_wasm":"missing.wasm"}}"#
            ),
        )
        .unwrap();
        let registry = PluginRegistry::discover(Some(dir.path()), None);
        assert!(registry.all().is_empty());
    }
}
