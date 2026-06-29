//! Embedded localhost HTTP server — REST + MCP endpoints for the Syllepsis search API.
//!
//! Started via `start()` after the Tauri setup hook. Stopped by calling `ServerHandle::stop()`.

pub mod docs;
pub mod mcp;
pub mod rest;

use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use std::net::TcpListener;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::search_api_config::SearchApiConfig;

/// Shared state accessible by all axum handlers.
#[derive(Clone)]
pub struct ApiState {
    pub handle: Arc<AppHandle>,
    /// Constant-time-compared bearer token.
    pub token: Arc<String>,
}

/// Held by the caller to stop the server gracefully.
pub struct ServerHandle {
    shutdown_tx: oneshot::Sender<()>,
}

impl ServerHandle {
    pub fn stop(self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Build and spawn the server. Returns a `ServerHandle` or an error if the port is in use.
pub fn start(app_handle: AppHandle, config: Arc<SearchApiConfig>) -> Result<ServerHandle, String> {
    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| format!("search API: could not bind to {addr}: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("search API: set_nonblocking: {e}"))?;

    let state = ApiState {
        handle: Arc::new(app_handle),
        token: Arc::new(config.token.clone().unwrap_or_default()),
    };

    let router = build_router(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tauri::async_runtime::spawn(async move {
        let tokio_listener = tokio::net::TcpListener::from_std(listener)
            .expect("convert TcpListener to tokio");
        axum::serve(tokio_listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok(ServerHandle { shutdown_tx })
}

/// Axum middleware that enforces the bearer token on `/api/*` and `/mcp`.
async fn auth_middleware(
    State(state): State<ApiState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let needs_auth = path.starts_with("/api") || path == "/mcp";
    if needs_auth {
        let bearer = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        match bearer {
            Some(t) if constant_time_eq(t, &state.token) => {}
            _ => {
                return Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header("content-type", "application/json")
                    .header("WWW-Authenticate", "Bearer realm=\"syllepsis-search\"")
                    .body(axum::body::Body::from(r#"{"error":"unauthorized"}"#))
                    .unwrap();
            }
        }
    }
    next.run(req).await
}

fn build_router(state: ApiState) -> Router {
    Router::new()
        // Docs — open (no auth check in middleware, path doesn't start with /api or /mcp)
        .route("/", get(docs::index_handler))
        .route("/openapi.json", get(docs::openapi_handler))
        // REST API
        .route("/api/search", get(rest::search_handler))
        .route("/api/notes/recent", get(rest::recent_handler))
        .route("/api/notes/core", get(rest::core_handler))
        .route("/api/notes/category/{cat}", get(rest::category_handler))
        .route("/api/notes/{id}", get(rest::note_handler))
        // MCP
        .route("/mcp", post(mcp::mcp_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state)
}

/// Constant-time string equality to mitigate timing attacks on the token.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.iter().zip(bb.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
