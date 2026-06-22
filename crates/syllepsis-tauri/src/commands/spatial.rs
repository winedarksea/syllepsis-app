//! Commands for Phase 5 spatial worlds: the worlds registry, the text→coordinate lookup table,
//! `loc:` token resolution, and building a world's overlay of pins and regions.

use tauri::State;

use syllepsis_core::app::spatial as app;
use syllepsis_core::model::World;
use syllepsis_core::spatial::{LookupEntry, Overlay, ResolvedLocation};

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

/// All worlds in the open book (`earth` first).
#[tauri::command]
pub fn list_worlds(state: State<AppState>) -> Result<Vec<World>, String> {
    with_book!(state, book, {
        app::list_worlds(book).map_err(|e| e.to_string())
    })
}

/// Create or overwrite a world (the built-in `earth` cannot be redefined).
#[tauri::command]
pub fn create_world(state: State<AppState>, world: World) -> Result<(), String> {
    with_book!(state, book, {
        app::create_world(book, world).map_err(|e| e.to_string())
    })
}

/// Delete a stored world by id.
#[tauri::command]
pub fn delete_world(state: State<AppState>, id: String) -> Result<(), String> {
    with_book!(state, book, {
        app::delete_world(book, &id).map_err(|e| e.to_string())
    })
}

/// Build the overlay (note/category pins and category regions) for one world.
#[tauri::command]
pub fn world_overlay(state: State<AppState>, world_id: String) -> Result<Overlay, String> {
    with_book!(state, book, {
        app::world_overlay(book, &world_id).map_err(|e| e.to_string())
    })
}

/// Every row of the text→coordinate lookup table.
#[tauri::command]
pub fn location_lookup(state: State<AppState>) -> Result<Vec<LookupEntry>, String> {
    with_book!(state, book, {
        app::location_lookup(book).map_err(|e| e.to_string())
    })
}

/// Insert or replace one lookup-table entry.
#[tauri::command]
pub fn set_location_lookup_entry(
    state: State<AppState>,
    entry: LookupEntry,
) -> Result<(), String> {
    with_book!(state, book, {
        app::set_location_lookup_entry(book, entry).map_err(|e| e.to_string())
    })
}

/// Resolve a raw `loc:` token to a concrete world + coordinate (for the location picker).
#[tauri::command]
pub fn resolve_location(state: State<AppState>, token: String) -> Result<ResolvedLocation, String> {
    with_book!(state, book, {
        app::resolve_location(book, &token).map_err(|e| e.to_string())
    })
}
