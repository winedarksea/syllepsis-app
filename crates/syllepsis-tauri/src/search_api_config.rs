//! Machine-local config for the embedded search API server.
//! Stored in `app_data_dir/search-api.json`; never synced (it's machine-local by design).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchApiConfig {
    pub enabled: bool,
    pub port: u16,
    /// Random bearer token; `None` until the user first enables the API (generated on enable).
    pub token: Option<String>,
}

impl Default for SearchApiConfig {
    fn default() -> Self {
        SearchApiConfig {
            enabled: false,
            port: 57384,
            token: None,
        }
    }
}

impl SearchApiConfig {
    pub fn path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("search-api.json")
    }

    pub fn load(app_data_dir: &Path) -> Self {
        let path = Self::path(app_data_dir);
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app_data_dir: &Path) -> Result<(), String> {
        let path = Self::path(app_data_dir);
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Generate a 32-byte cryptographically-random base64url token.
    pub fn generate_token(&mut self) {
        let mut bytes = [0u8; 32];
        // Read from the OS entropy source; on macOS/Linux this is /dev/urandom.
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            let _ = f.read_exact(&mut bytes);
        }
        self.token = Some(URL_SAFE_NO_PAD.encode(bytes));
    }
}
