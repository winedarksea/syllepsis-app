//! Hand-rolled MCP "Streamable HTTP" handler (JSON-RPC 2.0 over POST /mcp).
//!
//! Supports: initialize, tools/list, tools/call.
//! The five read-only tools delegate directly to the core query helpers.

use axum::{
    extract::State,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::Manager;

use syllepsis_core::{
    app::{
        commands::get_note,
        query::{core_notes, notes_by_category, recent_notes},
        search::search_with_query_embedding,
    },
    search::SearchFilter,
};

use crate::state::AppState;

use super::ApiState;

// ── JSON-RPC shapes ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn rpc_ok(id: Value, result: Value) -> Response {
    Json(JsonRpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }).into_response()
}

fn rpc_err(id: Value, code: i32, message: impl Into<String>) -> Response {
    Json(JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError { code, message: message.into() }),
    })
    .into_response()
}

// ── Tool catalogue ────────────────────────────────────────────────────────────

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "search",
                "description": "Full hybrid search (exact + BM25 + vector fused with RRF).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "q": { "type": "string", "description": "Search query" },
                        "n": { "type": "integer", "description": "Max results (default 10)" }
                    },
                    "required": ["q"]
                }
            },
            {
                "name": "get_note",
                "description": "Fetch a single note by its ULID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Note ULID" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "recent_notes",
                "description": "Most recently updated visible notes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "n": { "type": "integer", "description": "Max results (default 10)" }
                    }
                }
            },
            {
                "name": "core_notes",
                "description": "Notes marked as Core priority.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "notes_by_category",
                "description": "All visible notes in a category.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "category": { "type": "string", "description": "Category name (case-sensitive)" }
                    },
                    "required": ["category"]
                }
            }
        ]
    })
}

// ── Handler ───────────────────────────────────────────────────────────────────

pub async fn mcp_handler(
    State(api): State<ApiState>,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    let id = req.id.unwrap_or(Value::Null);
    let params = req.params.unwrap_or(Value::Object(Default::default()));

    match req.method.as_str() {
        "initialize" => rpc_ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "syllepsis-search", "version": "1.0" }
            }),
        ),

        "tools/list" => rpc_ok(id, tools_list()),

        "tools/call" => {
            let tool = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            dispatch_tool(id, tool, args, api).await
        }

        _ => rpc_err(id, -32601, format!("method not found: {}", req.method)),
    }
}

async fn dispatch_tool(id: Value, tool: String, args: Value, api: ApiState) -> Response {
    match tool.as_str() {
        "search" => {
            let q = match args.get("q").and_then(|v| v.as_str()) {
                Some(q) => q.to_string(),
                None => return rpc_err(id, -32602, "missing required argument: q"),
            };
            let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let id2 = id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = api.handle.state::<AppState>();
                let guard = state.book.lock().unwrap();
                let Some(book) = guard.as_ref() else {
                    return rpc_err(id, -32000, "no book is open");
                };
                let filter = SearchFilter::default();
                let embedding = state.local_ai.submit_query(book, q.clone()).ok();
                match search_with_query_embedding(book, &q, &filter, embedding.as_ref()) {
                    Ok(results) => {
                        let items: Vec<Value> = results
                            .hits
                            .iter()
                            .take(n)
                            .map(|hit| json!({
                                "id": hit.note_id,
                                "title": hit.title,
                                "summary": hit.summary,
                                "relevance": hit.relevance(),
                                "categories": hit.categories,
                                "updated": hit.updated.to_rfc3339()
                            }))
                            .collect();
                        let text = serde_json::to_string(&items).unwrap_or_default();
                        rpc_ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
                    }
                    Err(e) => rpc_err(id, -32000, e.to_string()),
                }
            })
            .await
            .unwrap_or_else(|e| rpc_err(id2, -32000, format!("worker failed: {e}")))
        }

        "get_note" => {
            let note_id = match args.get("id").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => return rpc_err(id, -32602, "missing required argument: id"),
            };
            let id2 = id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = api.handle.state::<AppState>();
                let guard = state.book.lock().unwrap();
                let Some(book) = guard.as_ref() else {
                    return rpc_err(id, -32000, "no book is open");
                };
                match get_note(book, &note_id) {
                    Ok(note) => {
                        let text = serde_json::to_string(&note).unwrap_or_default();
                        rpc_ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
                    }
                    Err(e) => rpc_err(id, -32000, e.to_string()),
                }
            })
            .await
            .unwrap_or_else(|e| rpc_err(id2, -32000, format!("worker failed: {e}")))
        }

        "recent_notes" => {
            let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let id2 = id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = api.handle.state::<AppState>();
                let guard = state.book.lock().unwrap();
                let Some(book) = guard.as_ref() else {
                    return rpc_err(id, -32000, "no book is open");
                };
                match recent_notes(book, n) {
                    Ok(notes) => {
                        let text = serde_json::to_string(&notes).unwrap_or_default();
                        rpc_ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
                    }
                    Err(e) => rpc_err(id, -32000, e.to_string()),
                }
            })
            .await
            .unwrap_or_else(|e| rpc_err(id2, -32000, format!("worker failed: {e}")))
        }

        "core_notes" => {
            let id2 = id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = api.handle.state::<AppState>();
                let guard = state.book.lock().unwrap();
                let Some(book) = guard.as_ref() else {
                    return rpc_err(id, -32000, "no book is open");
                };
                match core_notes(book) {
                    Ok(notes) => {
                        let text = serde_json::to_string(&notes).unwrap_or_default();
                        rpc_ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
                    }
                    Err(e) => rpc_err(id, -32000, e.to_string()),
                }
            })
            .await
            .unwrap_or_else(|e| rpc_err(id2, -32000, format!("worker failed: {e}")))
        }

        "notes_by_category" => {
            let category = match args.get("category").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => return rpc_err(id, -32602, "missing required argument: category"),
            };
            let id2 = id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = api.handle.state::<AppState>();
                let guard = state.book.lock().unwrap();
                let Some(book) = guard.as_ref() else {
                    return rpc_err(id, -32000, "no book is open");
                };
                match notes_by_category(book, &category) {
                    Ok(notes) => {
                        let text = serde_json::to_string(&notes).unwrap_or_default();
                        rpc_ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
                    }
                    Err(e) => rpc_err(id, -32000, e.to_string()),
                }
            })
            .await
            .unwrap_or_else(|e| rpc_err(id2, -32000, format!("worker failed: {e}")))
        }

        _ => rpc_err(id, -32601, format!("unknown tool: {tool}")),
    }
}
