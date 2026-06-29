//! Tauri commands for controlling the embedded search API server.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

use crate::{search_api_config::SearchApiConfig, server, state::AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchApiStatus {
    pub enabled: bool,
    pub port: u16,
    pub token: Option<String>,
    pub rest_url: String,
    pub mcp_url: String,
}

fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))
}

#[tauri::command]
pub fn search_api_status(app: AppHandle) -> Result<SearchApiStatus, String> {
    let dir = app_data_dir(&app)?;
    let cfg = SearchApiConfig::load(&dir);
    Ok(status_from_cfg(&cfg))
}

#[tauri::command]
pub fn set_search_api_enabled(
    app: AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<SearchApiStatus, String> {
    let dir = app_data_dir(&app)?;
    let mut cfg = SearchApiConfig::load(&dir);

    if enabled && cfg.token.is_none() {
        cfg.generate_token();
    }
    cfg.enabled = enabled;
    cfg.save(&dir)?;

    // Start or stop the server.
    let mut guard = state.search_api_server.lock().unwrap();
    if enabled {
        // Stop any running instance first (port may have changed).
        if let Some(handle) = guard.take() {
            handle.stop();
        }
        match server::start(app, Arc::new(cfg.clone())) {
            Ok(handle) => {
                *guard = Some(handle);
            }
            Err(e) => {
                // Revert enabled state so the UI knows it failed.
                cfg.enabled = false;
                cfg.save(&dir)?;
                return Err(e);
            }
        }
    } else if let Some(handle) = guard.take() {
        handle.stop();
    }

    Ok(status_from_cfg(&cfg))
}

#[tauri::command]
pub fn regenerate_search_api_token(
    app: AppHandle,
    state: State<AppState>,
) -> Result<SearchApiStatus, String> {
    let dir = app_data_dir(&app)?;
    let mut cfg = SearchApiConfig::load(&dir);
    cfg.generate_token();
    cfg.save(&dir)?;

    // Restart the server with the new token if it's running.
    if cfg.enabled {
        let mut guard = state.search_api_server.lock().unwrap();
        if let Some(handle) = guard.take() {
            handle.stop();
        }
        match server::start(app, Arc::new(cfg.clone())) {
            Ok(handle) => {
                *guard = Some(handle);
            }
            Err(e) => return Err(e),
        }
    }

    Ok(status_from_cfg(&cfg))
}

fn status_from_cfg(cfg: &SearchApiConfig) -> SearchApiStatus {
    let base = format!("http://127.0.0.1:{}", cfg.port);
    SearchApiStatus {
        enabled: cfg.enabled,
        port: cfg.port,
        token: cfg.token.clone(),
        rest_url: format!("{base}/api"),
        mcp_url: format!("{base}/mcp"),
    }
}
