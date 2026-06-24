//! Commands for the WASM plugin system: discovery, code-block rendering, and plugin-sourced
//! imports. The heavy lifting lives in `syllepsis_core::app::plugin` and the core plugin host; this
//! file is the Tauri-shaped shell around it (state access, file I/O, error stringification).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

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
    /// Persisted set of disabled plugin ids. Snapshot before passing to WASM to release the lock.
    pub disabled_ids: Mutex<HashSet<String>>,
    /// Where the disabled set is persisted across launches.
    prefs_path: PathBuf,
}

impl PluginRuntime {
    /// Discover and load all plugins from the bundled and user directories. Loads the disabled set
    /// from `prefs_path` if it exists (silently defaults to empty on any read/parse error).
    pub fn load(
        builtin_dir: Option<PathBuf>,
        user_dir: Option<PathBuf>,
        prefs_path: PathBuf,
    ) -> PluginRuntime {
        let disabled_ids: HashSet<String> = std::fs::read_to_string(&prefs_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();

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
        PluginRuntime { registry, host, disabled_ids: Mutex::new(disabled_ids), prefs_path }
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

/// Persist the disabled-ids set to `prefs_path` as a JSON array of strings.
fn save_disabled_ids(path: &PathBuf, ids: &HashSet<String>) -> Result<(), String> {
    let list: Vec<&String> = ids.iter().collect();
    let json = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("save plugin prefs: {e}"))
}

/// List every installed plugin (for the Settings panel and the editor's language map).
/// Each descriptor's `enabled` field reflects the current disabled set.
#[tauri::command]
pub fn list_plugins(plugins: State<PluginRuntime>) -> Vec<PluginDescriptor> {
    let disabled = plugins.disabled_ids.lock().unwrap();
    app_plugin::list_plugins(&plugins.registry, &disabled)
}

/// Enable or disable a plugin by id. The change takes effect immediately for all subsequent
/// render and import calls; the preference is persisted across launches.
#[tauri::command]
pub fn set_plugin_enabled(
    plugins: State<PluginRuntime>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut ids = plugins.disabled_ids.lock().unwrap();
    if enabled {
        ids.remove(&plugin_id);
    } else {
        ids.insert(plugin_id);
    }
    save_disabled_ids(&plugins.prefs_path, &ids)
}

/// Copy a `.wasm` plugin file into the user plugin directory. The plugin loads on next launch.
#[tauri::command]
pub fn install_user_plugin(app: AppHandle, source_path: String) -> Result<String, String> {
    let user_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("plugins");
    std::fs::create_dir_all(&user_dir).map_err(|e| format!("create plugins dir: {e}"))?;
    let source = std::path::Path::new(&source_path);
    let file_name = source
        .file_name()
        .ok_or_else(|| "invalid source path".to_string())?;
    let dest = user_dir.join(file_name);
    std::fs::copy(source, &dest).map_err(|e| format!("install plugin: {e}"))?;
    Ok(file_name.to_string_lossy().into_owned())
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
    // Snapshot disabled set before entering WASM (releases the lock before the call).
    let disabled = plugins.disabled_ids.lock().unwrap().clone();
    app_plugin::run_render_plugin(&plugins.host, &plugins.registry, &disabled, &language, &code)
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
    // Snapshot disabled set before entering WASM (releases the lock before the call).
    let disabled = plugins.disabled_ids.lock().unwrap().clone();
    app_plugin::import_via_plugin(
        &plugins.host,
        &plugins.registry,
        &disabled,
        &plugin_id,
        &bytes,
        &options,
    )
    .map_err(|e| e.to_string())
}
