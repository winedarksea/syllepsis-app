//! Versioned, synced embedding records for one note.
//!
//! The sidecar is deliberately independent of Markdown and SQLite. Markdown remains the user's
//! source document, while SQLite is a disposable local projection. The binary format keeps the
//! synced artifact compact and avoids YAML float churn.

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::embeddings::Embedding;
use crate::error::{CoreError, CoreResult};
use crate::model::Note;

const MAGIC: &[u8; 8] = b"SYLVEC01";
pub const SIDECAR_SCHEMA_VERSION: u32 = 1;
pub const INPUT_POLICY_VERSION: &str = "embeddinggemma-head-tail-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelFingerprint {
    pub model_id: String,
    pub model_revision: String,
    pub quantization: String,
    pub dimensions: usize,
    pub input_policy_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredEmbedding {
    pub source_hash: [u8; 32],
    pub vector: Embedding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteEmbeddingSidecar {
    pub note_ulid: String,
    pub generated_at_unix_ms: i64,
    pub model: EmbeddingModelFingerprint,
    pub summary: Option<StoredEmbedding>,
    pub full_note: Option<StoredEmbedding>,
}

impl NoteEmbeddingSidecar {
    pub fn new(
        note: &Note,
        model: EmbeddingModelFingerprint,
        summary: Option<Embedding>,
        full_note: Option<Embedding>,
    ) -> NoteEmbeddingSidecar {
        NoteEmbeddingSidecar {
            note_ulid: note.id.ulid().to_string(),
            generated_at_unix_ms: Utc::now().timestamp_millis(),
            model,
            summary: summary.map(|mut vector| {
                vector.normalize();
                StoredEmbedding {
                    source_hash: summary_source_hash(note),
                    vector,
                }
            }),
            full_note: full_note.map(|mut vector| {
                vector.normalize();
                StoredEmbedding {
                    source_hash: full_note_source_hash(note),
                    vector,
                }
            }),
        }
    }

    pub fn is_compatible_with(&self, expected: &EmbeddingModelFingerprint) -> bool {
        &self.model == expected
    }

    pub fn summary_is_fresh(&self, note: &Note) -> bool {
        if note.summary.trim().is_empty() {
            return self.summary.is_none();
        }
        self.summary
            .as_ref()
            .is_some_and(|stored| stored.source_hash == summary_source_hash(note))
    }

    pub fn full_note_is_fresh(&self, note: &Note) -> bool {
        let content_is_empty = note.body.trim().is_empty() && note.summary.trim().is_empty();
        if note.title.trim().is_empty() && content_is_empty {
            return self.full_note.is_none();
        }
        self.full_note
            .as_ref()
            .is_some_and(|stored| stored.source_hash == full_note_source_hash(note))
    }
}

pub fn summary_source_hash(note: &Note) -> [u8; 32] {
    hash_fields(&[note.title.as_bytes(), note.summary.as_bytes()])
}

pub fn full_note_source_hash(note: &Note) -> [u8; 32] {
    let content = if note.body.trim().is_empty() { note.summary.as_bytes() } else { note.body.as_bytes() };
    hash_fields(&[note.title.as_bytes(), content])
}

fn hash_fields(fields: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    hasher.finalize().into()
}

pub fn write_sidecar_atomic(path: &Path, sidecar: &NoteEmbeddingSidecar) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = encode(sidecar)?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, bytes)?;
    if let Err(error) = fs::rename(&temp, path) {
        if path.exists() {
            fs::remove_file(path)?;
            fs::rename(&temp, path)?;
        } else {
            return Err(error.into());
        }
    }
    Ok(())
}

pub fn read_sidecar(path: &Path) -> CoreResult<NoteEmbeddingSidecar> {
    decode(&fs::read(path)?)
}

pub fn encode(sidecar: &NoteEmbeddingSidecar) -> CoreResult<Vec<u8>> {
    validate_sidecar(sidecar)?;
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&SIDECAR_SCHEMA_VERSION.to_le_bytes());
    out.extend_from_slice(&sidecar.generated_at_unix_ms.to_le_bytes());
    write_string(&mut out, &sidecar.note_ulid)?;
    write_string(&mut out, &sidecar.model.model_id)?;
    write_string(&mut out, &sidecar.model.model_revision)?;
    write_string(&mut out, &sidecar.model.quantization)?;
    out.extend_from_slice(&(sidecar.model.dimensions as u32).to_le_bytes());
    write_string(&mut out, &sidecar.model.input_policy_version)?;
    write_stored_embedding(&mut out, sidecar.summary.as_ref())?;
    write_stored_embedding(&mut out, sidecar.full_note.as_ref())?;
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> CoreResult<NoteEmbeddingSidecar> {
    let mut cursor = Cursor::new(bytes);
    let mut magic = [0u8; 8];
    cursor.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(CoreError::parse("embedding sidecar", "invalid magic"));
    }
    let version = read_u32(&mut cursor)?;
    if version != SIDECAR_SCHEMA_VERSION {
        return Err(CoreError::parse(
            "embedding sidecar",
            format!("unsupported schema version {version}"),
        ));
    }
    let generated_at_unix_ms = read_i64(&mut cursor)?;
    let note_ulid = read_string(&mut cursor)?;
    let model_id = read_string(&mut cursor)?;
    let model_revision = read_string(&mut cursor)?;
    let quantization = read_string(&mut cursor)?;
    let dimensions = read_u32(&mut cursor)? as usize;
    let input_policy_version = read_string(&mut cursor)?;
    let summary = read_stored_embedding(&mut cursor, dimensions)?;
    let full_note = read_stored_embedding(&mut cursor, dimensions)?;
    if cursor.position() != bytes.len() as u64 {
        return Err(CoreError::parse(
            "embedding sidecar",
            "unexpected trailing bytes",
        ));
    }
    let sidecar = NoteEmbeddingSidecar {
        note_ulid,
        generated_at_unix_ms,
        model: EmbeddingModelFingerprint {
            model_id,
            model_revision,
            quantization,
            dimensions,
            input_policy_version,
        },
        summary,
        full_note,
    };
    validate_sidecar(&sidecar)?;
    Ok(sidecar)
}

fn validate_sidecar(sidecar: &NoteEmbeddingSidecar) -> CoreResult<()> {
    if sidecar.model.dimensions == 0 {
        return Err(CoreError::parse(
            "embedding sidecar",
            "zero vector dimensions",
        ));
    }
    for stored in [sidecar.summary.as_ref(), sidecar.full_note.as_ref()]
        .into_iter()
        .flatten()
    {
        if stored.vector.len() != sidecar.model.dimensions {
            return Err(CoreError::parse(
                "embedding sidecar",
                "vector dimensions do not match model fingerprint",
            ));
        }
    }
    Ok(())
}

fn write_stored_embedding(out: &mut Vec<u8>, stored: Option<&StoredEmbedding>) -> CoreResult<()> {
    match stored {
        None => out.push(0),
        Some(stored) => {
            out.push(1);
            out.extend_from_slice(&stored.source_hash);
            out.extend_from_slice(&(stored.vector.len() as u32).to_le_bytes());
            for value in &stored.vector.0 {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    Ok(())
}

fn read_stored_embedding(
    cursor: &mut Cursor<&[u8]>,
    expected_dimensions: usize,
) -> CoreResult<Option<StoredEmbedding>> {
    let mut present = [0u8; 1];
    cursor.read_exact(&mut present)?;
    if present[0] == 0 {
        return Ok(None);
    }
    if present[0] != 1 {
        return Err(CoreError::parse(
            "embedding sidecar",
            "invalid optional-vector marker",
        ));
    }
    let mut source_hash = [0u8; 32];
    cursor.read_exact(&mut source_hash)?;
    let dimensions = read_u32(cursor)? as usize;
    if dimensions != expected_dimensions {
        return Err(CoreError::parse(
            "embedding sidecar",
            "stored vector has unexpected dimensions",
        ));
    }
    let mut values = Vec::with_capacity(dimensions);
    for _ in 0..dimensions {
        let mut bytes = [0u8; 4];
        cursor.read_exact(&mut bytes)?;
        values.push(f32::from_le_bytes(bytes));
    }
    Ok(Some(StoredEmbedding {
        source_hash,
        vector: Embedding::new(values),
    }))
}

fn write_string(out: &mut Vec<u8>, value: &str) -> CoreResult<()> {
    let bytes = value.as_bytes();
    let length = u32::try_from(bytes.len())
        .map_err(|_| CoreError::Model("embedding sidecar string is too large".into()))?;
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> CoreResult<String> {
    let length = read_u32(cursor)? as usize;
    let mut bytes = vec![0u8; length];
    cursor.read_exact(&mut bytes)?;
    String::from_utf8(bytes)
        .map_err(|error| CoreError::parse("embedding sidecar", error.to_string()))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> CoreResult<u32> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i64(cursor: &mut Cursor<&[u8]>) -> CoreResult<i64> {
    let mut bytes = [0u8; 8];
    cursor.read_exact(&mut bytes)?;
    Ok(i64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    fn fingerprint() -> EmbeddingModelFingerprint {
        EmbeddingModelFingerprint {
            model_id: "embeddinggemma-300m".into(),
            model_revision: "abc".into(),
            quantization: "q4".into(),
            dimensions: 3,
            input_policy_version: INPUT_POLICY_VERSION.into(),
        }
    }

    #[test]
    fn binary_sidecar_round_trips() {
        let mut note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        note.summary = "Summary".into();
        note.body = "Body".into();
        let sidecar = NoteEmbeddingSidecar::new(
            &note,
            fingerprint(),
            Some(Embedding::new(vec![1.0, 0.0, 0.0])),
            Some(Embedding::new(vec![0.0, 1.0, 0.0])),
        );
        assert_eq!(decode(&encode(&sidecar).unwrap()).unwrap(), sidecar);
    }

    #[test]
    fn title_changes_invalidate_both_vectors() {
        let mut note = Note::new(ObjectType::Note, "Title", "syllepsis_001");
        note.summary = "Summary".into();
        note.body = "Body".into();
        let sidecar = NoteEmbeddingSidecar::new(
            &note,
            fingerprint(),
            Some(Embedding::new(vec![1.0, 0.0, 0.0])),
            Some(Embedding::new(vec![0.0, 1.0, 0.0])),
        );
        note.title = "Changed".into();
        assert!(!sidecar.summary_is_fresh(&note));
        assert!(!sidecar.full_note_is_fresh(&note));
    }
}
