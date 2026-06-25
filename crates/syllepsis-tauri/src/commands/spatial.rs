//! Commands for Phase 5 spatial worlds: the worlds registry, the text→coordinate lookup table,
//! `loc:` token resolution, and building a world's overlay of pins and regions.

use tauri::State;

use syllepsis_core::app::spatial as app;
use syllepsis_core::app::spatial::{CreateImageWorldRequest, WorldDeletionImpact};
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

#[tauri::command]
pub fn create_image_world(
    state: State<AppState>,
    request: CreateImageWorldRequest,
) -> Result<World, String> {
    with_book!(state, book, {
        app::create_image_world(book, request).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn world_deletion_impact(
    state: State<AppState>,
    id: String,
) -> Result<WorldDeletionImpact, String> {
    with_book!(state, book, {
        app::world_deletion_impact(book, &id).map_err(|e| e.to_string())
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
pub fn set_location_lookup_entry(state: State<AppState>, entry: LookupEntry) -> Result<(), String> {
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

/// Serve an image world's backdrop as a self-contained `data:` URL the overlay view draws behind
/// its pins/regions. `None` ⇒ no backdrop to draw (geo world, unset, or asset not on disk yet).
/// A data URL keeps this dependency-free of the Tauri asset protocol and its CSP scope.
#[tauri::command]
pub fn world_backdrop(state: State<AppState>, world_id: String) -> Result<Option<String>, String> {
    with_book!(state, book, {
        let Some(backdrop) = app::world_backdrop(book, &world_id).map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&backdrop.path).map_err(|e| e.to_string())?;
        Ok(Some(format!(
            "data:{};base64,{}",
            backdrop.mime,
            base64_encode(&bytes)
        )))
    })
}

/// Standard base64 of `bytes` (no line wrapping). Local helper so serving a backdrop image does
/// not pull in a crate for ~15 lines of well-known encoding.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
