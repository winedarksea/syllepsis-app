//! Commands for category CRUD.

use tauri::State;

use syllepsis_core::app::commands as app;
use syllepsis_core::model::Category;

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

/// All categories defined in the open book.
#[tauri::command]
pub fn all_categories(state: State<AppState>) -> Result<Vec<Category>, String> {
    with_book!(state, book, {
        app::all_categories(book).map_err(|e| e.to_string())
    })
}

/// Create or overwrite a category.
#[tauri::command]
pub fn create_category(state: State<AppState>, category: Category) -> Result<(), String> {
    with_book!(state, book, {
        app::create_category(book, category).map_err(|e| e.to_string())
    })
}
