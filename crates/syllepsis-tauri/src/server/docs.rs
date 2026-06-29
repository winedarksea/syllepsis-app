//! Self-documenting endpoints: GET / and GET /openapi.json (no auth required).

use axum::response::{IntoResponse, Json};
use serde_json::json;

pub async fn index_handler() -> impl IntoResponse {
    Json(json!({
        "name": "Syllepsis Search API",
        "description": "Localhost-only read-only REST + MCP RAG API. All /api/* and /mcp routes require Authorization: Bearer <token>.",
        "endpoints": {
            "GET /": "This index",
            "GET /openapi.json": "OpenAPI 3.1 specification",
            "GET /api/search?q=<query>&n=<count>": "Hybrid search results",
            "GET /api/notes/{id}": "Single note by ID",
            "GET /api/notes/recent?n=<count>": "Most recently updated notes",
            "GET /api/notes/core": "Notes with Core priority",
            "GET /api/notes/category/{cat}": "Notes in a category",
            "POST /mcp": "MCP JSON-RPC 2.0 endpoint (tools: search, get_note, recent_notes, core_notes, notes_by_category)"
        }
    }))
}

pub async fn openapi_handler() -> impl IntoResponse {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Syllepsis Search API",
            "description": "Localhost read-only search API for the open Syllepsis book.",
            "version": "1.0.0"
        },
        "servers": [{ "url": "http://127.0.0.1:{port}", "description": "Local Syllepsis instance" }],
        "security": [{ "BearerAuth": [] }],
        "components": {
            "securitySchemes": {
                "BearerAuth": { "type": "http", "scheme": "bearer" }
            }
        },
        "paths": {
            "/api/search": {
                "get": {
                    "summary": "Hybrid search",
                    "parameters": [
                        { "name": "q", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "n", "in": "query", "schema": { "type": "integer", "default": 10 } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Search hits",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": { "type": "string" },
                                                "title": { "type": "string" },
                                                "summary": { "type": "string" },
                                                "relevance": { "type": "number" },
                                                "score": { "type": "number" },
                                                "object_type": { "type": "string" },
                                                "categories": { "type": "array", "items": { "type": "string" } },
                                                "updated": { "type": "string", "format": "date-time" }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "No book open" }
                    }
                }
            },
            "/api/notes/{id}": {
                "get": {
                    "summary": "Get a single note",
                    "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Note DTO" },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Note not found" },
                        "409": { "description": "No book open" }
                    }
                }
            },
            "/api/notes/recent": {
                "get": {
                    "summary": "Recently updated notes",
                    "parameters": [{ "name": "n", "in": "query", "schema": { "type": "integer", "default": 10 } }],
                    "responses": {
                        "200": { "description": "Array of note DTOs" },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "No book open" }
                    }
                }
            },
            "/api/notes/core": {
                "get": {
                    "summary": "Notes with Core priority",
                    "responses": {
                        "200": { "description": "Array of note DTOs" },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "No book open" }
                    }
                }
            },
            "/api/notes/category/{cat}": {
                "get": {
                    "summary": "Notes in a category",
                    "parameters": [{ "name": "cat", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": {
                        "200": { "description": "Array of note DTOs" },
                        "401": { "description": "Unauthorized" },
                        "409": { "description": "No book open" }
                    }
                }
            },
            "/mcp": {
                "post": {
                    "summary": "MCP JSON-RPC 2.0 endpoint",
                    "description": "Tools: search, get_note, recent_notes, core_notes, notes_by_category",
                    "responses": {
                        "200": { "description": "JSON-RPC response" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            }
        }
    }))
}
