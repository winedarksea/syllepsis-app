//! The WASM runtime that actually *runs* plugins, behind the `extism` feature — the optional half
//! of the seam, exactly as [`crate::crdt::loro_backend`] is the optional CRDT backend.
//!
//! One [`PluginHost`] owns the loaded [`extism::Plugin`] instances and dispatches the two hook
//! calls: `run_import` (bytes → text) and `run_render` (lang+code → html). It also registers the
//! note-write host functions (`create_note`, `replace_body`) that the contract exposes to plugins;
//! they forward into [`super::note_write`]. The two built-in plugins don't call them, but they are
//! wired so a future plugin can.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use extism::{convert::Json, Function, Manifest, Plugin, UserData, Wasm, PTR};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::storage::Book;

use super::note_write::{self, CreateNoteInput};
use super::registry::PluginRegistry;

/// Plugin output for an import call: the extracted plain text the importer will chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOutput {
    pub text: String,
}

/// Plugin input for a render call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInput {
    pub language: String,
    pub code: String,
}

/// Plugin output for a render call: HTML to be sanitized and shown in place of the code block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOutput {
    pub html: String,
}

/// The note-write host functions need to reach the open book. We hold its root path (not the live
/// [`Book`]) so a host-function call opens its own handle — sidestepping any lock re-entrancy with
/// the caller that is mid-`run_*`. Updated whenever the app opens/closes a book.
#[derive(Default)]
struct HostContext {
    book_root: Option<PathBuf>,
}

// `create_note(json) -> note_id`: create a note from a plugin-supplied JSON payload.
extism::host_fn!(host_create_note(user_data: HostContext; input: Json<CreateNoteInput>) -> String {
    let ctx = user_data.get()?;
    let book_root = ctx.lock().unwrap().book_root.clone();
    let root = book_root.ok_or_else(|| CoreError::Plugin("no book is open".to_string()))?;
    let book = Book::open(root)?;
    let Json(input) = input;
    Ok(note_write::create_note(&book, &input)?)
});

// `replace_body(json{note_id, body})`: replace a note's body wholesale.
extism::host_fn!(host_replace_body(user_data: HostContext; input: Json<ReplaceBodyInput>) -> String {
    let ctx = user_data.get()?;
    let book_root = ctx.lock().unwrap().book_root.clone();
    let root = book_root.ok_or_else(|| CoreError::Plugin("no book is open".to_string()))?;
    let book = Book::open(root)?;
    let Json(input) = input;
    note_write::replace_body(&book, &input.note_id, &input.body)?;
    Ok(input.note_id)
});

#[derive(Debug, Clone, Deserialize)]
struct ReplaceBodyInput {
    note_id: String,
    body: String,
}

/// The loaded set of runnable plugins. Each [`Plugin`] is behind a `Mutex` because
/// [`Plugin::call`] takes `&mut self`; calls are short, so per-plugin locking is fine.
pub struct PluginHost {
    plugins: HashMap<String, Mutex<Plugin>>,
    ctx: UserData<HostContext>,
}

impl PluginHost {
    /// Load every plugin in `registry` into a runnable host. A plugin that fails to instantiate is
    /// logged and skipped — one bad plugin must not sink the rest.
    pub fn load(registry: &PluginRegistry) -> PluginHost {
        let ctx = UserData::new(HostContext::default());
        let mut plugins = HashMap::new();
        for installed in registry.all() {
            match build_plugin(&installed.wasm_path, &ctx) {
                Ok(plugin) => {
                    plugins.insert(installed.manifest.id.clone(), Mutex::new(plugin));
                }
                Err(error) => {
                    tracing::warn!(
                        id = %installed.manifest.id,
                        %error,
                        "failed to load plugin into WASM host"
                    );
                }
            }
        }
        PluginHost { plugins, ctx }
    }

    /// Point the note-write host functions at the currently open book (or `None` when closed).
    pub fn set_book_root(&self, root: Option<PathBuf>) {
        if let Ok(ctx) = self.ctx.get() {
            ctx.lock().unwrap().book_root = root;
        }
    }

    /// Run an import-source plugin over raw file bytes, returning the extracted text.
    pub fn run_import(&self, plugin_id: &str, bytes: &[u8]) -> CoreResult<String> {
        let mut plugin = self.lock_plugin(plugin_id)?;
        let Json(out): Json<ImportOutput> = plugin
            .call("import", bytes)
            .map_err(|e| CoreError::Plugin(format!("plugin '{plugin_id}' import failed: {e}")))?;
        Ok(out.text)
    }

    /// Run a code-block-renderer plugin over a fenced block, returning (unsanitized) HTML.
    pub fn run_render(&self, plugin_id: &str, language: &str, code: &str) -> CoreResult<String> {
        let mut plugin = self.lock_plugin(plugin_id)?;
        let input = RenderInput {
            language: language.to_string(),
            code: code.to_string(),
        };
        let Json(out): Json<RenderOutput> = plugin
            .call("render", Json(input))
            .map_err(|e| CoreError::Plugin(format!("plugin '{plugin_id}' render failed: {e}")))?;
        Ok(out.html)
    }

    fn lock_plugin(&self, plugin_id: &str) -> CoreResult<std::sync::MutexGuard<'_, Plugin>> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| CoreError::Plugin(format!("plugin '{plugin_id}' is not loaded")))?;
        Ok(plugin.lock().expect("plugin mutex poisoned"))
    }
}

/// Instantiate one Extism plugin from its `.wasm`, wiring in the note-write host functions.
fn build_plugin(wasm_path: &PathBuf, ctx: &UserData<HostContext>) -> CoreResult<Plugin> {
    let manifest = Manifest::new([Wasm::file(wasm_path)]);
    let functions = [
        Function::new("create_note", [PTR], [PTR], ctx.clone(), host_create_note),
        Function::new("replace_body", [PTR], [PTR], ctx.clone(), host_replace_body),
    ];
    Plugin::new(&manifest, functions, true)
        .map_err(|e| CoreError::Plugin(format!("instantiate {}: {e}", wasm_path.display())))
}
