//! Shared access to synced embedding sidecars.
//!
//! Consumers load vectors through this module so search, diagnostics, and graph analysis agree on
//! compatibility and staleness. Missing or incompatible records are represented as zero/no-signal
//! vectors rather than triggering inference on the read path.

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::config::EmbeddingConfig;
use crate::embeddings::note::NoteVectors;
use crate::embeddings::sidecar::{
    read_sidecar, write_sidecar_atomic, EmbeddingModelFingerprint, NoteEmbeddingSidecar,
    INPUT_POLICY_VERSION,
};
use crate::embeddings::EmbeddingProvider;
use crate::error::{CoreError, CoreResult};
use crate::model::Note;
use crate::onnx::manifest::{self, Quantization};
use crate::storage::{layout, Book, NoteStore};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingCoverage {
    pub total_notes: usize,
    pub fresh_notes: usize,
    pub stale_notes: usize,
    pub missing_notes: usize,
    pub incompatible_notes: usize,
    pub blocked_notes: usize,
}

#[derive(Debug, Clone)]
pub struct LoadedEmbeddingCorpus {
    pub vectors: Vec<NoteVectors>,
    pub coverage: EmbeddingCoverage,
}

pub fn configured_model_fingerprint(
    config: &EmbeddingConfig,
) -> CoreResult<EmbeddingModelFingerprint> {
    let model = manifest::builtin(&config.model_id)
        .ok_or_else(|| CoreError::Model(format!("unknown embedding model {}", config.model_id)))?;
    let dimensions = config
        .matryoshka_dims
        .filter(|dimensions| *dimensions > 0)
        .unwrap_or(model.hidden_size)
        .min(model.hidden_size);
    Ok(EmbeddingModelFingerprint {
        model_id: model.id,
        model_revision: model.revision,
        quantization: quantization_name(model.quantization).to_string(),
        dimensions,
        input_policy_version: INPUT_POLICY_VERSION.to_string(),
    })
}

pub fn load_embedding_corpus(book: &Book, notes: &[Note]) -> CoreResult<LoadedEmbeddingCorpus> {
    let expected = configured_model_fingerprint(&book.config.embedding)?;
    let mut coverage = EmbeddingCoverage {
        total_notes: notes.len(),
        ..EmbeddingCoverage::default()
    };
    let mut vectors = Vec::with_capacity(notes.len());

    for note in notes {
        let path = layout::embedding_sidecar_path(&book.root, &note.id);
        let sidecar = match read_sidecar(&path) {
            Ok(sidecar) => sidecar,
            Err(CoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                coverage.missing_notes += 1;
                vectors.push(NoteVectors::missing(note, expected.dimensions));
                continue;
            }
            Err(error) => {
                tracing::warn!(note = %note.id, error = %error, "ignoring unreadable embedding sidecar");
                coverage.incompatible_notes += 1;
                vectors.push(NoteVectors::missing(note, expected.dimensions));
                continue;
            }
        };
        if sidecar.note_ulid != note.id.ulid() || !sidecar.is_compatible_with(&expected) {
            coverage.incompatible_notes += 1;
            vectors.push(NoteVectors::missing(note, expected.dimensions));
            continue;
        }
        let stale = !sidecar.summary_is_fresh(note) || !sidecar.full_note_is_fresh(note);
        if stale {
            coverage.stale_notes += 1;
        } else {
            coverage.fresh_notes += 1;
        }
        vectors.push(NoteVectors::from_sidecar(note, &sidecar, stale));
    }

    Ok(LoadedEmbeddingCorpus { vectors, coverage })
}

pub fn generate_note_sidecar(
    book: &Book,
    provider: &dyn EmbeddingProvider,
    note: &Note,
) -> CoreResult<NoteEmbeddingSidecar> {
    let expected = configured_model_fingerprint(&book.config.embedding)?;
    if provider.name() != expected.model_id {
        return Err(CoreError::Model(format!(
            "active embedding provider {} does not match configured model {}",
            provider.name(),
            expected.model_id
        )));
    }
    if provider.dimensions() != expected.dimensions {
        return Err(CoreError::Model(format!(
            "active embedding provider emitted {} dimensions; expected {}",
            provider.dimensions(),
            expected.dimensions
        )));
    }
    let path = layout::embedding_sidecar_path(&book.root, &note.id);
    let existing = read_sidecar(&path)
        .ok()
        .filter(|sidecar| sidecar.is_compatible_with(&expected));
    let summary = match existing.as_ref() {
        Some(sidecar) if sidecar.summary_is_fresh(note) => {
            sidecar.summary.as_ref().map(|stored| stored.vector.clone())
        }
        _ => provider.embed_note_summary(note)?,
    };
    let full_note = match existing.as_ref() {
        Some(sidecar) if sidecar.full_note_is_fresh(note) => sidecar
            .full_note
            .as_ref()
            .map(|stored| stored.vector.clone()),
        _ => provider.embed_full_note(note)?,
    };
    let sidecar = NoteEmbeddingSidecar::new(note, expected, summary, full_note);
    write_sidecar_atomic(&path, &sidecar)?;
    Ok(sidecar)
}

pub fn stale_or_missing_note_ids(book: &Book, notes: &[Note]) -> CoreResult<Vec<String>> {
    let expected = configured_model_fingerprint(&book.config.embedding)?;
    Ok(notes
        .iter()
        .filter(|note| {
            let path = layout::embedding_sidecar_path(&book.root, &note.id);
            let Ok(sidecar) = read_sidecar(&path) else {
                return true;
            };
            !sidecar.is_compatible_with(&expected)
                || !sidecar.summary_is_fresh(note)
                || !sidecar.full_note_is_fresh(note)
        })
        .map(|note| note.id.to_string())
        .collect())
}

pub fn note_embedding_is_stale(book: &Book, note: &Note) -> CoreResult<bool> {
    let expected = configured_model_fingerprint(&book.config.embedding)?;
    let path = layout::embedding_sidecar_path(&book.root, &note.id);
    let Ok(sidecar) = read_sidecar(&path) else {
        return Ok(true);
    };
    Ok(!sidecar.is_compatible_with(&expected)
        || !sidecar.summary_is_fresh(note)
        || !sidecar.full_note_is_fresh(note))
}

/// Deterministic preference used when two devices generated the same note's sidecar concurrently.
/// Current-content/model matches outrank recency; the content hash is the final convergence tie.
pub fn sidecar_preference_rank(book: &Book, bytes: &[u8]) -> (bool, u8, i64, String) {
    let content_hash = format!("{:x}", sha2::Sha256::digest(bytes));
    let Ok(sidecar) = crate::embeddings::sidecar::decode(bytes) else {
        return (false, 0, i64::MIN, content_hash);
    };
    let compatible = configured_model_fingerprint(&book.config.embedding)
        .map(|expected| sidecar.is_compatible_with(&expected))
        .unwrap_or(false);
    let note = book.store.read_all_notes().ok().and_then(|notes| {
        notes
            .into_iter()
            .find(|note| note.id.ulid() == sidecar.note_ulid)
    });
    let fresh_fields = note
        .as_ref()
        .map(|note| {
            u8::from(sidecar.summary_is_fresh(note)) + u8::from(sidecar.full_note_is_fresh(note))
        })
        .unwrap_or(0);
    (
        compatible,
        fresh_fields,
        sidecar.generated_at_unix_ms,
        content_hash,
    )
}

fn quantization_name(quantization: Quantization) -> &'static str {
    match quantization {
        Quantization::Fp32 => "fp32",
        Quantization::Fp16 => "fp16",
        Quantization::Int8 => "int8",
        Quantization::Q4 => "q4",
        Quantization::Q4F16 => "q4f16",
    }
}

#[cfg(test)]
pub(crate) fn write_test_sidecars(book: &Book, notes: &[Note]) {
    use crate::embeddings::HashingEmbedder;

    let fingerprint = configured_model_fingerprint(&book.config.embedding).unwrap();
    let provider = HashingEmbedder::new(fingerprint.dimensions);
    for note in notes {
        let (summary, full_note) = provider.embed_note_fields(note).unwrap();
        let sidecar = NoteEmbeddingSidecar::new(note, fingerprint.clone(), summary, full_note);
        write_sidecar_atomic(
            &layout::embedding_sidecar_path(&book.root, &note.id),
            &sidecar,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::HashingEmbedder;
    use crate::model::ObjectType;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn missing_sidecar_is_reported_without_embedding() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path().join("book"), "Test").unwrap();
        let note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        let loaded = load_embedding_corpus(&book, &[note]).unwrap();
        assert_eq!(loaded.coverage.missing_notes, 1);
        assert_eq!(loaded.vectors[0].centroid.magnitude(), 0.0);
    }

    #[test]
    fn incompatible_provider_cannot_write_canonical_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path().join("book"), "Test").unwrap();
        let note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        let provider = HashingEmbedder::new(256);
        assert!(generate_note_sidecar(&book, &provider, &note).is_err());
    }

    struct CountingProvider {
        summary_calls: AtomicUsize,
        full_calls: AtomicUsize,
    }

    impl crate::embeddings::ProviderInfo for CountingProvider {
        fn name(&self) -> &str {
            manifest::EMBEDDINGGEMMA_ID
        }
    }

    impl EmbeddingProvider for CountingProvider {
        fn dimensions(&self) -> usize {
            256
        }

        fn embed(&self, _text: &str) -> crate::embeddings::Embedding {
            crate::embeddings::Embedding::zeros(256)
        }

        fn embed_note_summary(
            &self,
            note: &Note,
        ) -> CoreResult<Option<crate::embeddings::Embedding>> {
            self.summary_calls.fetch_add(1, Ordering::SeqCst);
            Ok((!note.summary.is_empty()).then(|| unit_vector(0)))
        }

        fn embed_full_note(
            &self,
            _note: &Note,
        ) -> CoreResult<Option<crate::embeddings::Embedding>> {
            self.full_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(unit_vector(1)))
        }
    }

    fn unit_vector(index: usize) -> crate::embeddings::Embedding {
        let mut values = vec![0.0; 256];
        values[index] = 1.0;
        crate::embeddings::Embedding::new(values)
    }

    #[test]
    fn summary_change_reuses_fresh_full_note_vector() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path().join("book"), "Test").unwrap();
        let mut note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        note.summary = "first".into();
        note.body = "body".into();
        let provider = CountingProvider {
            summary_calls: AtomicUsize::new(0),
            full_calls: AtomicUsize::new(0),
        };
        generate_note_sidecar(&book, &provider, &note).unwrap();
        note.summary = "second".into();
        generate_note_sidecar(&book, &provider, &note).unwrap();
        assert_eq!(provider.summary_calls.load(Ordering::SeqCst), 2);
        assert_eq!(provider.full_calls.load(Ordering::SeqCst), 1);
    }
}
