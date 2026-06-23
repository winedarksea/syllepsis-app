//! Commands for style card CRUD (stored as JSON files in `_style_cards/` inside the book).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;

/// A style card stored on disk: the card data plus a unique id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleCardEntry {
    pub id: String,
    pub version: u32,
    pub short_description: String,
    pub field: String,
    pub tenor: String,
    pub mode: String,
    pub density: String,
    pub texture: String,
    pub organization: String,
    #[serde(default)]
    pub exemplars: Vec<StyleExemplar>,
    #[serde(default)]
    pub source_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleExemplar {
    pub text: String,
    #[serde(default)]
    pub note: String,
}

fn cards_dir(book_root: &std::path::Path) -> PathBuf {
    book_root.join("_style_cards")
}

fn card_path(book_root: &std::path::Path, id: &str) -> PathBuf {
    cards_dir(book_root).join(format!("{id}.json"))
}

fn ensure_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create _style_cards dir: {e}"))
}

/// List all style cards in the open book.
#[tauri::command]
pub fn list_style_cards(state: State<AppState>) -> Result<Vec<StyleCardEntry>, String> {
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let dir = cards_dir(&book.root);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut cards = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        match serde_json::from_str::<StyleCardEntry>(&text) {
            Ok(card) => cards.push(card),
            Err(e) => tracing::warn!("skipping malformed style card {:?}: {e}", path),
        }
    }
    cards.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cards)
}

/// Save (create or update) a style card.
#[tauri::command]
pub fn save_style_card(state: State<AppState>, card: StyleCardEntry) -> Result<StyleCardEntry, String> {
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let dir = cards_dir(&book.root);
    ensure_dir(&dir)?;
    let id = if card.id.is_empty() {
        format!("sc-{}", ulid::Ulid::new().to_string().to_lowercase())
    } else {
        card.id.clone()
    };
    let saved = StyleCardEntry { id: id.clone(), ..card };
    let text = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    std::fs::write(card_path(&book.root, &id), text).map_err(|e| e.to_string())?;
    Ok(saved)
}

/// Delete a style card by id.
#[tauri::command]
pub fn delete_style_card(state: State<AppState>, id: String) -> Result<(), String> {
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let path = card_path(&book.root, &id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
