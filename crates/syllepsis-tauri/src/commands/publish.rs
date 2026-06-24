//! Commands for publishing & serving (Phase 6, platform-infra.md): export a read-only static site
//! and refresh the private-content git exclusion.

use std::path::Path;

use tauri::State;

use syllepsis_core::app::publish::{self as app, GitignoreReport, PublishReport};
use syllepsis_core::app::plugin as app_plugin;

use crate::commands::plugins::PluginRuntime;
use crate::state::AppState;

macro_rules! with_book {
    ($state:expr, $book:ident, $body:expr) => {{
        let guard = $state.book.lock().unwrap();
        match guard.as_ref() {
            None => Err("no book is open".to_string()),
            Some($book) => $body,
        }
    }};
}

/// Render the book's public view (private content excluded) to `index.html` under `out_dir`.
/// Plugin-claimed code blocks are rendered via the loaded WASM plugin host.
#[tauri::command]
pub fn publish_site(
    state: State<AppState>,
    plugins: State<PluginRuntime>,
    out_dir: String,
) -> Result<PublishReport, String> {
    with_book!(state, book, {
        plugins.host.set_book_root(Some(book.root.clone()));
        let disabled = plugins.disabled_ids.lock().unwrap().clone();
        app::publish_site(book, Path::new(&out_dir), &|lang, code| {
            app_plugin::run_render_plugin(&plugins.host, &plugins.registry, &disabled, lang, code).ok()
        })
        .map_err(|e| e.to_string())
    })
}

/// Rewrite the managed `.gitignore` block to exclude private notes and private categories.
#[tauri::command]
pub fn refresh_private_gitignore(state: State<AppState>) -> Result<GitignoreReport, String> {
    with_book!(state, book, {
        app::refresh_private_gitignore(book).map_err(|e| e.to_string())
    })
}
