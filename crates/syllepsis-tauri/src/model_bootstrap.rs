//! Make the canonical embedder available without requiring user setup.

use std::path::{Path, PathBuf};

use syllepsis_core::onnx::{
    builtin, download_missing, HttpModelFetcher, ModelCache, ModelManifest, EMBEDDINGGEMMA_ID,
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
        let bundled_cache = ModelCache::new(resource_dir.join("models"));
        if bundled_cache.is_cached(&manifest) {
            match copy_bundled_model(&bundled_cache, &destination_cache, &manifest) {
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

fn embedding_manifest() -> Result<ModelManifest, String> {
    builtin(EMBEDDINGGEMMA_ID).ok_or_else(|| "EmbeddingGemma manifest is unavailable".to_string())
}

fn copy_bundled_model(
    source: &ModelCache,
    destination: &ModelCache,
    manifest: &ModelManifest,
) -> Result<(), String> {
    std::fs::create_dir_all(destination.model_dir(manifest)).map_err(|error| error.to_string())?;
    for file in &manifest.files {
        let destination_path = destination.file_path(manifest, file);
        if destination_path
            .metadata()
            .ok()
            .is_some_and(|metadata| file.size_bytes == Some(metadata.len()))
        {
            continue;
        }
        copy_file_atomically(&source.file_path(manifest, file), &destination_path)?;
    }
    destination
        .is_cached(manifest)
        .then_some(())
        .ok_or_else(|| "bundled EmbeddingGemma files did not produce a complete cache".into())
}

fn copy_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    let temporary = temporary_copy_path(destination);
    let result = (|| {
        // A hard link avoids consuming another ~207 MB when the application resources and app
        // data are on the same volume. Cross-volume and restricted filesystems fall back to copy.
        if std::fs::hard_link(source, &temporary).is_err() {
            std::fs::copy(source, &temporary).map_err(|error| {
                format!(
                    "copy bundled model {} to {}: {error}",
                    source.display(),
                    temporary.display()
                )
            })?;
        }
        std::fs::rename(&temporary, destination)
            .map_err(|error| format!("install bundled model {}: {error}", destination.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
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
    use syllepsis_core::onnx::manifest::FileRole;
    use syllepsis_core::onnx::ModelFile;

    #[test]
    fn bundled_copy_installs_a_complete_small_manifest() {
        let source_directory = tempfile::tempdir().unwrap();
        let destination_directory = tempfile::tempdir().unwrap();
        let mut manifest = embedding_manifest().unwrap();
        manifest.files = vec![ModelFile {
            repo_path: "model.bin".into(),
            role: FileRole::Weights,
            sha256: None,
            size_bytes: Some(4),
        }];
        let source = ModelCache::new(source_directory.path());
        let destination = ModelCache::new(destination_directory.path());
        std::fs::create_dir_all(source.model_dir(&manifest)).unwrap();
        std::fs::write(source.file_path(&manifest, &manifest.files[0]), b"data").unwrap();

        copy_bundled_model(&source, &destination, &manifest).unwrap();

        assert!(destination.is_cached(&manifest));
    }
}
