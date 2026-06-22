//! The real [`ModelFetcher`]: a blocking HTTP download from Hugging Face (feature `onnx`).
//!
//! This is the side-effecting half of [`download`](super::download) that the pure planning and
//! orchestration are tested without. It streams a response straight to disk via a `.part`
//! temp file that is renamed only on full success, so an interrupted download never leaves a
//! truncated file that the cache would later mistake for "present". Redirects (Hugging Face
//! serves LFS weights from a CDN via 302) are followed by reqwest's default policy.

use std::fs::File;

use crate::error::{CoreError, CoreResult};
use crate::onnx::download::{DownloadItem, ModelFetcher};

/// Downloads model files over HTTPS with a blocking client.
pub struct HttpModelFetcher {
    client: reqwest::blocking::Client,
}

impl HttpModelFetcher {
    /// Build a fetcher with a long timeout suited to multi-hundred-MB weight files.
    pub fn new() -> CoreResult<HttpModelFetcher> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("syllepsis/0.1 (+https://github.com/)")
            // Weights are large; allow a generous read timeout but keep connect short.
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| CoreError::Model(format!("http client init failed: {e}")))?;
        Ok(HttpModelFetcher { client })
    }
}

impl ModelFetcher for HttpModelFetcher {
    fn fetch(&self, item: &DownloadItem) -> CoreResult<()> {
        let mut response = self
            .client
            .get(&item.url)
            .send()
            .map_err(|e| CoreError::Model(format!("GET {} failed: {e}", item.url)))?
            .error_for_status()
            .map_err(|e| CoreError::Model(format!("GET {} returned error: {e}", item.url)))?;

        // Stream to a sibling .part file, then atomically rename into place.
        let part = item.dest.with_extension("part");
        let mut file = File::create(&part)
            .map_err(|e| CoreError::Model(format!("create {}: {e}", part.display())))?;
        response
            .copy_to(&mut file)
            .map_err(|e| CoreError::Model(format!("write {}: {e}", part.display())))?;
        std::fs::rename(&part, &item.dest)
            .map_err(|e| CoreError::Model(format!("finalize {}: {e}", item.dest.display())))?;
        Ok(())
    }
}
