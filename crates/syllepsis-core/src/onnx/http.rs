//! The real [`ModelFetcher`]: a blocking HTTP download from Hugging Face (feature `onnx`).
//!
//! This is the side-effecting half of [`download`](super::download) that the pure planning and
//! orchestration are tested without. It streams a response straight to disk via a `.part`
//! temp file that is renamed only on full success, so an interrupted download never leaves a
//! truncated file that the cache would later mistake for "present". Redirects (Hugging Face
//! serves LFS weights from a CDN via 302) are followed by reqwest's default policy.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use crate::error::{CoreError, CoreResult};
use crate::onnx::download::{DownloadItem, ModelFetcher};

const MAX_DOWNLOAD_ATTEMPTS: usize = 5;
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(1);

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
        let part = item.dest.with_extension("part");
        remove_oversized_partial_file(&part, item.size_bytes)?;

        let mut last_error = None;
        for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
            let existing_bytes = part.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            if item
                .size_bytes
                .is_some_and(|expected_bytes| existing_bytes == expected_bytes)
            {
                break;
            }

            match self.download_attempt(item, &part, existing_bytes) {
                Ok(()) => {
                    let downloaded_bytes =
                        part.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                    if item
                        .size_bytes
                        .is_none_or(|expected_bytes| downloaded_bytes == expected_bytes)
                    {
                        break;
                    }
                    last_error = Some(format!(
                        "incomplete response: downloaded {downloaded_bytes} of {} bytes",
                        item.size_bytes.unwrap()
                    ));
                }
                Err(error) => last_error = Some(error),
            }

            if attempt < MAX_DOWNLOAD_ATTEMPTS {
                std::thread::sleep(INITIAL_RETRY_DELAY * attempt as u32);
            }
        }

        if let Some(expected_bytes) = item.size_bytes {
            let downloaded_bytes = part.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            if downloaded_bytes != expected_bytes {
                return Err(CoreError::Model(format!(
                    "download {} failed after {MAX_DOWNLOAD_ATTEMPTS} attempts \
                     ({downloaded_bytes}/{expected_bytes} bytes retained for resume): {}",
                    item.url,
                    last_error.unwrap_or_else(|| "incomplete response".to_string())
                )));
            }
        } else if let Some(error) = last_error {
            return Err(CoreError::Model(format!(
                "download {} failed after {MAX_DOWNLOAD_ATTEMPTS} attempts: {error}",
                item.url
            )));
        }

        std::fs::rename(&part, &item.dest)
            .map_err(|e| CoreError::Model(format!("finalize {}: {e}", item.dest.display())))?;
        Ok(())
    }
}

impl HttpModelFetcher {
    fn download_attempt(
        &self,
        item: &DownloadItem,
        part: &std::path::Path,
        existing_bytes: u64,
    ) -> Result<(), String> {
        let mut request = self.client.get(&item.url);
        if existing_bytes > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing_bytes}-"));
        }
        let mut response = request
            .send()
            .map_err(|error| format!("GET failed: {error}"))?
            .error_for_status()
            .map_err(|error| format!("GET returned error: {error}"))?;

        let server_honored_range =
            existing_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(server_honored_range)
            .truncate(!server_honored_range)
            .open(part)
            .map_err(|error| format!("open {}: {error}", part.display()))?;

        std::io::copy(&mut response, &mut file)
            .map_err(|error| format!("stream response into {}: {error}", part.display()))?;
        file.flush()
            .map_err(|error| format!("flush {}: {error}", part.display()))
    }
}

fn remove_oversized_partial_file(
    part: &std::path::Path,
    expected_size_bytes: Option<u64>,
) -> CoreResult<()> {
    let Some(expected_size_bytes) = expected_size_bytes else {
        return Ok(());
    };
    let Ok(metadata) = part.metadata() else {
        return Ok(());
    };
    if metadata.len() > expected_size_bytes {
        std::fs::remove_file(part)?;
    }
    Ok(())
}
