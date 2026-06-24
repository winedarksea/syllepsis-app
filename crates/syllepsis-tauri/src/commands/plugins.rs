//! Commands for the WASM plugin system: discovery, code-block rendering, and plugin-sourced
//! imports. The heavy lifting lives in `syllepsis_core::app::plugin` and the core plugin host; this
//! file is the Tauri-shaped shell around it (state access, file I/O, error stringification).

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use syllepsis_core::app::plugin::{self as app_plugin, PluginDescriptor};
use syllepsis_core::app::text_import::{TextImportOptions, TextImportPreview};
use syllepsis_core::plugin::{PluginHost, PluginRegistry};

use crate::state::AppState;

/// Dev/test override for the built-in plugin directory (mirrors `SYLLEPSIS_MODEL_CACHE`). When set,
/// it replaces the bundled-resource lookup so a freshly built `.wasm` can be pointed at directly.
pub const PLUGIN_DIR_ENV_VAR: &str = "SYLLEPSIS_PLUGIN_DIR";

/// The app-level plugin runtime: the discovered registry plus the loaded WASM host. Built once at
/// startup and shared as Tauri state (its methods take `&self`).
pub struct PluginRuntime {
    pub registry: PluginRegistry,
    pub host: PluginHost,
}

impl PluginRuntime {
    /// Discover and load all plugins from the bundled and user directories.
    pub fn load(builtin_dir: Option<PathBuf>, user_dir: Option<PathBuf>) -> PluginRuntime {
        let registry = PluginRegistry::discover(builtin_dir.as_deref(), user_dir.as_deref());
        if registry.all().is_empty() {
            tracing::info!("no plugins discovered");
        } else {
            for plugin in registry.all() {
                tracing::info!(
                    id = %plugin.manifest.id,
                    version = %plugin.manifest.version,
                    "discovered plugin"
                );
            }
        }
        let host = PluginHost::load(&registry);
        PluginRuntime { registry, host }
    }
}

/// Resolve the built-in (bundled resource) and user plugin directories for this app install.
///
/// Resolution order for the built-in dir:
///   1. `SYLLEPSIS_PLUGIN_DIR` env var (explicit override for any environment)
///   2. `<resource_dir>/plugins` (production bundle)
///   3. `<workspace-root>/plugins/dist` (debug builds only, found via `CARGO_MANIFEST_DIR`)
pub fn plugin_dirs(app: &AppHandle) -> (Option<PathBuf>, Option<PathBuf>) {
    let builtin = if let Some(v) = std::env::var_os(PLUGIN_DIR_ENV_VAR).filter(|v| !v.is_empty()) {
        Some(PathBuf::from(v))
    } else {
        let resource_candidate = app
            .path()
            .resource_dir()
            .ok()
            .map(|dir| dir.join("plugins"));
        // Use the resource dir only if it actually exists (it won't in a `cargo tauri dev` run).
        match resource_candidate.filter(|p| p.is_dir()) {
            Some(p) => Some(p),
            None => dev_builtin_fallback(),
        }
    };

    let user = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("plugins"));
    (builtin, user)
}

/// In debug builds, locate `plugins/dist` relative to the workspace root using the crate's
/// compile-time manifest path. Returns `None` in release builds or if the directory is absent.
fn dev_builtin_fallback() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        // CARGO_MANIFEST_DIR = <workspace>/crates/syllepsis-tauri at compile time.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidate = manifest.join("../../plugins/dist");
        std::fs::canonicalize(candidate).ok().filter(|p| p.is_dir())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// List every installed plugin (for the Settings panel and the editor's language map).
#[tauri::command]
pub fn list_plugins(plugins: State<PluginRuntime>) -> Vec<PluginDescriptor> {
    app_plugin::list_plugins(&plugins.registry)
}

/// Render a fenced code block with the renderer plugin that claims `language`. Returns the
/// plugin's HTML, which the frontend sanitizes before displaying.
#[tauri::command]
pub fn run_render_plugin(
    state: State<AppState>,
    plugins: State<PluginRuntime>,
    language: String,
    code: String,
) -> Result<String, String> {
    // Keep the note-write host functions pointed at the open book (harmless if unused here).
    if let Some(book) = state.book.lock().unwrap().as_ref() {
        plugins.host.set_book_root(Some(book.root.clone()));
    }
    app_plugin::run_render_plugin(&plugins.host, &plugins.registry, &language, &code)
        .map_err(|e| e.to_string())
}

/// Run an import-source plugin over a chosen file and return a text-import preview, so the rest of
/// the Note Importer flow (chunk + commit) is identical to a pasted/text-file import.
#[tauri::command]
pub fn preview_plugin_import(
    state: State<AppState>,
    plugins: State<PluginRuntime>,
    plugin_id: String,
    path: String,
    options: TextImportOptions,
) -> Result<TextImportPreview, String> {
    if let Some(book) = state.book.lock().unwrap().as_ref() {
        plugins.host.set_book_root(Some(book.root.clone()));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("read import file {path}: {e}"))?;
    app_plugin::import_via_plugin(
        &plugins.host,
        &plugins.registry,
        &plugin_id,
        &bytes,
        &options,
    )
    .map_err(|e| e.to_string())
}
