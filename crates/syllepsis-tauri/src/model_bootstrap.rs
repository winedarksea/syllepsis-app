//! Make the canonical embedder available without requiring user setup.

use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader},
    path::{Path, PathBuf},
};

use syllepsis_core::onnx::{
    builtin, download_missing, HttpModelFetcher, ModelCache, ModelManifest, BUNDLED_LLM_ID,
    EMBEDDINGGEMMA_ID,
};
use tauri::{AppHandle, Manager};

use crate::state::{models_root_from_app_data, AppState};

pub fn provision_default_embedding_model(app: &AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir: {error}"))?;
    let destination_root = models_root_from_app_data(&app_data_dir);
    let manifest = embedding_manifest()?;
    let destination_cache = ModelCache::new(&destination_root);
    if destination_cache.is_cached(&manifest) {
        return Ok(());
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let archive_path = bundled_model_archive_path(&resource_dir);
        if archive_path.is_file() {
            match extract_bundled_model_archive(&archive_path, &destination_cache, &manifest) {
                Ok(()) => {
                    resume_embedding_queue(app);
                    return Ok(());
                }
                Err(error) => tracing::error!(
                    error = %error,
                    "bundled EmbeddingGemma installation failed; downloading replacement"
                ),
            }
        }
    }

    spawn_download_fallback(app.clone(), destination_root);
    Ok(())
}

/// Install the bundled local LLM on first launch. A changed manifest file name intentionally
/// makes an older model cache incomplete, so this also migrates desktop-model installations to
/// the QAT-mobile export without altering the stable configured model id.
pub fn provision_bundled_local_llm(app: &AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir: {error}"))?;
    let destination_root = models_root_from_app_data(&app_data_dir);
    let manifest = local_llm_manifest()?;
    let destination_cache = ModelCache::new(&destination_root);
    if destination_cache.is_cached(&manifest) {
        return Ok(());
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let archive_path = bundled_model_archive_path(&resource_dir);
        if archive_path.is_file() {
            return extract_bundled_model_archive(&archive_path, &destination_cache, &manifest);
        }
    }

    spawn_llm_download_fallback(destination_root);
    Ok(())
}

fn embedding_manifest() -> Result<ModelManifest, String> {
    builtin(EMBEDDINGGEMMA_ID).ok_or_else(|| "EmbeddingGemma manifest is unavailable".to_string())
}

fn local_llm_manifest() -> Result<ModelManifest, String> {
    builtin(BUNDLED_LLM_ID).ok_or_else(|| "bundled local LLM manifest is unavailable".to_string())
}

fn bundled_model_archive_path(resource_dir: &Path) -> PathBuf {
    resource_dir.join("model-archives").join("models.tar.xz")
}

/// Extract only one model from the app resource archive. Files are staged beside their final
/// paths and renamed only after every expected byte count has been written, so an interrupted
/// first launch cannot leave a cache that looks complete.
fn extract_bundled_model_archive(
    archive_path: &Path,
    destination: &ModelCache,
    manifest: &ModelManifest,
) -> Result<(), String> {
    let mut temporary_paths = Vec::with_capacity(manifest.files.len());
    let result = (|| {
        std::fs::create_dir_all(destination.model_dir(manifest))
            .map_err(|error| error.to_string())?;
        let archive_file = File::open(archive_path).map_err(|error| error.to_string())?;
        let decoder = xz2::read::XzDecoder::new(BufReader::new(archive_file));
        let mut archive = tar::Archive::new(decoder);

        for entry in archive.entries().map_err(|error| error.to_string())? {
            let mut entry = entry.map_err(|error| error.to_string())?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let archive_entry_path = entry.path().map_err(|error| error.to_string())?;
            let Some(model_file) = manifest
                .files
                .iter()
                .find(|file| archive_entry_matches_model_file(&archive_entry_path, manifest, file))
            else {
                continue;
            };

            let destination_path = destination.file_path(manifest, model_file);
            let temporary_path = temporary_copy_path(&destination_path);
            let _ = std::fs::remove_file(&temporary_path);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(|error| error.to_string())?;
            let written = io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
            if model_file
                .size_bytes
                .is_some_and(|expected| expected != written)
            {
                return Err(format!(
                    "bundled archive entry {} has an unexpected size",
                    model_file.file_name()
                ));
            }
            temporary_paths.push((temporary_path, destination_path));
        }

        if temporary_paths.len() != manifest.files.len() {
            return Err(format!(
                "bundled archive does not contain every file for {}",
                manifest.id
            ));
        }
        for (temporary_path, destination_path) in &temporary_paths {
            std::fs::rename(temporary_path, destination_path).map_err(|error| error.to_string())?;
        }
        destination
            .is_cached(manifest)
            .then_some(())
            .ok_or_else(|| "bundled archive extraction did not produce a complete cache".into())
    })();
    if result.is_err() {
        for (temporary_path, _) in temporary_paths {
            let _ = std::fs::remove_file(temporary_path);
        }
    }
    result
}

fn archive_entry_matches_model_file(
    archive_path: &Path,
    manifest: &ModelManifest,
    file: &syllepsis_core::onnx::ModelFile,
) -> bool {
    let components: Vec<_> = archive_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| *component != ".")
        .collect();
    components == [manifest.id.as_str(), file.file_name()]
}

fn temporary_copy_path(destination: &Path) -> PathBuf {
    destination.with_extension(format!(
        "{}.installing",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("model")
    ))
}

fn spawn_download_fallback(app: AppHandle, destination_root: PathBuf) {
    std::thread::Builder::new()
        .name("syllepsis-model-bootstrap".into())
        .spawn(move || {
            let result = (|| {
                let manifest = embedding_manifest()?;
                let fetcher = HttpModelFetcher::new().map_err(|error| error.to_string())?;
                download_missing(&ModelCache::new(destination_root), &manifest, &fetcher)
                    .map_err(|error| error.to_string())?;
                Ok::<(), String>(())
            })();
            match result {
                Ok(()) => resume_embedding_queue(&app),
                Err(error) => tracing::error!(
                    error = %error,
                    "automatic EmbeddingGemma provisioning failed"
                ),
            }
        })
        .expect("start embedding model bootstrap");
}

fn spawn_llm_download_fallback(destination_root: PathBuf) {
    std::thread::Builder::new()
        .name("syllepsis-llm-model-bootstrap".into())
        .spawn(move || {
            let result = (|| {
                let manifest = local_llm_manifest()?;
                let fetcher = HttpModelFetcher::new().map_err(|error| error.to_string())?;
                download_missing(&ModelCache::new(destination_root), &manifest, &fetcher)
                    .map_err(|error| error.to_string())?;
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                tracing::error!(error = %error, "automatic local LLM provisioning failed");
            }
        })
        .expect("start local LLM model bootstrap");
}

fn resume_embedding_queue(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.invalidate_graph_corpus();
    let guard = state.book.lock().unwrap();
    if let Some(book) = guard.as_ref() {
        let _ = state.local_ai.enqueue_all_stale(book, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use syllepsis_core::onnx::manifest::FileRole;
    use syllepsis_core::onnx::ModelFile;

    #[test]
    fn bundled_archive_installs_a_complete_small_manifest() {
        let archive_directory = tempfile::tempdir().unwrap();
        let destination_directory = tempfile::tempdir().unwrap();
        let mut manifest = embedding_manifest().unwrap();
        manifest.files = vec![ModelFile {
            repo_path: "model.bin".into(),
            role: FileRole::Weights,
            sha256: None,
            size_bytes: Some(4),
        }];
        let destination = ModelCache::new(destination_directory.path());
        let archive_path = archive_directory.path().join("models.tar.xz");
        let archive_file = File::create(&archive_path).unwrap();
        let encoder = xz2::write::XzEncoder::new(archive_file, 6);
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                format!("./{}/model.bin", manifest.id),
                &b"data"[..],
            )
            .unwrap();
        let mut encoder = archive.into_inner().unwrap();
        encoder.flush().unwrap();
        encoder.finish().unwrap();

        extract_bundled_model_archive(&archive_path, &destination, &manifest).unwrap();

        assert!(destination.is_cached(&manifest));
    }
}
