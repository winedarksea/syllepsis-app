//! REST handlers for the search API.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use syllepsis_core::{
    app::{
        commands::get_note,
        query::{core_notes, notes_by_category, recent_notes},
        search::search_with_query_embedding,
    },
    search::{SearchFilter, SearchHit},
};

use crate::state::AppState;

use super::ApiState;

// ── Compact search hit ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ApiSearchHit {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub relevance: f32,
    pub score: f32,
    pub object_type: String,
    pub categories: Vec<String>,
    pub updated: String,
}

impl ApiSearchHit {
    fn from_hit(hit: &SearchHit) -> Self {
        ApiSearchHit {
            id: hit.note_id.clone(),
            title: hit.title.clone(),
            summary: hit.summary.clone(),
            relevance: hit.relevance(),
            score: hit.score,
            object_type: serde_json::to_value(&hit.object_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            categories: hit.categories.clone(),
            updated: hit.updated.to_rfc3339(),
        }
    }
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn no_book() -> Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({ "error": "no book is open" })),
    )
        .into_response()
}

fn internal(msg: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": msg.to_string() })),
    )
        .into_response()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_n")]
    pub n: usize,
}

#[derive(Deserialize)]
pub struct CountParams {
    #[serde(default = "default_n")]
    pub n: usize,
}

fn default_n() -> usize {
    10
}

pub async fn search_handler(
    State(api): State<ApiState>,
    Query(params): Query<SearchParams>,
) -> Response {
    let q = params.q.clone();
    let n = params.n;
    tauri::async_runtime::spawn_blocking(move || {
        let state = api.handle.state::<AppState>();
        let guard = state.book.lock().unwrap();
        let Some(book) = guard.as_ref() else {
            return no_book();
        };
        let filter = SearchFilter::default();
        let query_embedding = state.local_ai.submit_query(book, q.clone()).ok();
        match search_with_query_embedding(book, &q, &filter, query_embedding.as_ref()) {
            Ok(results) => {
                let hits: Vec<ApiSearchHit> = results
                    .hits
                    .iter()
                    .take(n)
                    .map(ApiSearchHit::from_hit)
                    .collect();
                Json(hits).into_response()
            }
            Err(e) => internal(e),
        }
    })
    .await
    .unwrap_or_else(|e| internal(format!("worker failed: {e}")))
}

pub async fn recent_handler(
    State(api): State<ApiState>,
    Query(params): Query<CountParams>,
) -> Response {
    let n = params.n;
    tauri::async_runtime::spawn_blocking(move || {
        let state = api.handle.state::<AppState>();
        let guard = state.book.lock().unwrap();
        let Some(book) = guard.as_ref() else {
            return no_book();
        };
        match recent_notes(book, n) {
            Ok(notes) => Json(notes).into_response(),
            Err(e) => internal(e),
        }
    })
    .await
    .unwrap_or_else(|e| internal(format!("worker failed: {e}")))
}

pub async fn core_handler(State(api): State<ApiState>) -> Response {
    tauri::async_runtime::spawn_blocking(move || {
        let state = api.handle.state::<AppState>();
        let guard = state.book.lock().unwrap();
        let Some(book) = guard.as_ref() else {
            return no_book();
        };
        match core_notes(book) {
            Ok(notes) => Json(notes).into_response(),
            Err(e) => internal(e),
        }
    })
    .await
    .unwrap_or_else(|e| internal(format!("worker failed: {e}")))
}

pub async fn category_handler(State(api): State<ApiState>, Path(cat): Path<String>) -> Response {
    tauri::async_runtime::spawn_blocking(move || {
        let state = api.handle.state::<AppState>();
        let guard = state.book.lock().unwrap();
        let Some(book) = guard.as_ref() else {
            return no_book();
        };
        match notes_by_category(book, &cat) {
            Ok(notes) => Json(notes).into_response(),
            Err(e) => internal(e),
        }
    })
    .await
    .unwrap_or_else(|e| internal(format!("worker failed: {e}")))
}

pub async fn note_handler(State(api): State<ApiState>, Path(id): Path<String>) -> Response {
    tauri::async_runtime::spawn_blocking(move || {
        let state = api.handle.state::<AppState>();
        let guard = state.book.lock().unwrap();
        let Some(book) = guard.as_ref() else {
            return no_book();
        };
        match get_note(book, &id) {
            Ok(note) => Json(note).into_response(),
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    })
    .await
    .unwrap_or_else(|e| internal(format!("worker failed: {e}")))
}
