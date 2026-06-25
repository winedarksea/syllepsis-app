//! Turning a [`Note`] (and a set of notes) into the vectors search and diagnostics consume.
//!
//! A note is embedded as several **parts** — its title+summary "header" and one vector per
//! body chunk — plus a single **centroid** (the normalized mean of those parts). The two serve
//! different jobs:
//! - *parts* let a query match the single most relevant passage (max similarity), so a long
//!   note is found by a question about any one section of it.
//! - the *centroid* is the note's one representative point, used for note-to-note similarity
//!   (the related carousel, duplicate/blind-spot diagnostics) and for averaging into a
//!   category vector.

use crate::config::EmbeddingConfig;
use crate::embeddings::chunk::chunk_text;
use crate::embeddings::provider::EmbeddingProvider;
use crate::embeddings::sidecar::NoteEmbeddingSidecar;
use crate::embeddings::vector::Embedding;
use crate::id::NoteId;
use crate::model::{Category, Note};

/// The multi-resolution embedding of one note.
#[derive(Debug, Clone)]
pub struct NoteVectors {
    pub note_id: NoteId,
    /// One representative unit vector (mean of `parts`); zero if the note has no text.
    pub centroid: Embedding,
    /// Header + per-chunk vectors, for matching a query against the closest passage.
    pub parts: Vec<Embedding>,
    pub summary: Option<Embedding>,
    pub full_note: Option<Embedding>,
    pub stale: bool,
}

impl NoteVectors {
    /// The strongest match between `query` and any part of this note (0 if the note is empty).
    pub fn best_similarity(&self, query: &Embedding) -> f32 {
        self.parts
            .iter()
            .map(|p| p.cosine_similarity(query))
            .fold(0.0_f32, f32::max)
    }

    pub fn missing(note: &Note, dimensions: usize) -> NoteVectors {
        NoteVectors {
            note_id: note.id.clone(),
            centroid: Embedding::zeros(dimensions),
            parts: Vec::new(),
            summary: None,
            full_note: None,
            stale: false,
        }
    }

    pub fn from_sidecar(note: &Note, sidecar: &NoteEmbeddingSidecar, stale: bool) -> NoteVectors {
        let summary = sidecar.summary.as_ref().map(|stored| stored.vector.clone());
        let full_note = sidecar
            .full_note
            .as_ref()
            .map(|stored| stored.vector.clone());
        let centroid = full_note
            .clone()
            .or_else(|| summary.clone())
            .unwrap_or_else(|| Embedding::zeros(sidecar.model.dimensions));
        let parts = [summary.clone(), full_note.clone()]
            .into_iter()
            .flatten()
            .collect();
        NoteVectors {
            note_id: note.id.clone(),
            centroid,
            parts,
            summary,
            full_note,
            stale,
        }
    }
}

/// Embed one note into its [`NoteVectors`]. The header (title + summary) is always the first
/// part so even a body-less quick-capture has a vector.
pub fn embed_note(
    provider: &dyn EmbeddingProvider,
    note: &Note,
    cfg: &EmbeddingConfig,
) -> NoteVectors {
    let mut parts = Vec::new();

    let header = format!("{} {}", note.title, note.summary);
    if !header.trim().is_empty() {
        parts.push(provider.embed(&header));
    }
    for chunk in chunk_text(&note.body, cfg) {
        parts.push(provider.embed(&chunk.text));
    }

    let centroid =
        Embedding::average(parts.iter()).unwrap_or_else(|| Embedding::zeros(provider.dimensions()));

    NoteVectors {
        note_id: note.id.clone(),
        centroid,
        parts,
        summary: None,
        full_note: None,
        stale: false,
    }
}

/// Embed every note once. The natural building block for the search engine and the category
/// averager, which both need every note's centroid.
pub fn embed_notes(
    provider: &dyn EmbeddingProvider,
    notes: &[Note],
    cfg: &EmbeddingConfig,
) -> Vec<NoteVectors> {
    notes.iter().map(|n| embed_note(provider, n, cfg)).collect()
}

/// The vector for a category: the normalized mean of its member notes' centroids
/// (object-types.md — a category's vector is the average of its notes). Returns `None` if the
/// category has no embeddable members, so callers can skip empty facets rather than store a
/// meaningless zero vector.
pub fn category_vector(
    category: &Category,
    note_vectors: &[(&Note, &NoteVectors)],
) -> Option<Embedding> {
    let members: Vec<&Embedding> = note_vectors
        .iter()
        .filter(|(note, _)| note.categories.iter().any(|c| c == &category.name))
        .map(|(_, vectors)| &vectors.centroid)
        .filter(|e| e.magnitude() > f32::EPSILON)
        .collect();
    Embedding::average(members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::hashing::HashingEmbedder;
    use crate::model::ObjectType;

    fn cfg() -> EmbeddingConfig {
        EmbeddingConfig {
            chunk_token_limit: 4,
            chunk_overlap_tokens: 1,
            dimensions: 256,
            ..Default::default()
        }
    }

    fn note(title: &str, body: &str, cats: &[&str]) -> Note {
        let mut n = Note::new(ObjectType::Note, title, "syllepsis_001");
        n.body = body.to_string();
        n.categories = cats.iter().map(|c| c.to_string()).collect();
        n
    }

    #[test]
    fn long_body_produces_multiple_parts() {
        let provider = HashingEmbedder::new(256);
        let n = note("title", "one two three four five six seven eight", &[]);
        let v = embed_note(&provider, &n, &cfg());
        // header + at least two body chunks (8 words, window 4 stride 3).
        assert!(v.parts.len() >= 3, "got {} parts", v.parts.len());
        assert!((v.centroid.magnitude() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn empty_note_has_zero_centroid_and_no_parts() {
        let provider = HashingEmbedder::new(256);
        let mut n = note("", "", &[]);
        n.title.clear();
        let v = embed_note(&provider, &n, &cfg());
        assert!(v.parts.is_empty());
        assert!(v.centroid.magnitude() < 1e-6);
    }

    #[test]
    fn best_similarity_finds_the_right_passage() {
        let provider = HashingEmbedder::new(512);
        let n = note(
            "house",
            "kitchen electrical panel breaker. garden roses soil compost watering.",
            &[],
        );
        let v = embed_note(&provider, &n, &cfg());
        let q = provider.embed("compost watering garden");
        // The closest body chunk should beat the average centroid for a passage query.
        assert!(v.best_similarity(&q) >= v.centroid.cosine_similarity(&q));
    }

    #[test]
    fn category_vector_averages_only_members() {
        let provider = HashingEmbedder::new(512);
        let in_a = note("a1", "kitchen wiring panel", &["electrical"]);
        let in_a2 = note("a2", "breaker outlet circuit", &["electrical"]);
        let outside = note("b", "roses garden soil", &["garden"]);
        let notes = vec![in_a.clone(), in_a2.clone(), outside.clone()];
        let vectors = embed_notes(&provider, &notes, &cfg());
        let pairs: Vec<(&Note, &NoteVectors)> = notes.iter().zip(&vectors).collect();

        let cat = Category::new("electrical");
        let cv = category_vector(&cat, &pairs).expect("members exist");
        // Closer to an electrical note than to the garden note.
        let elec = &vectors[0].centroid;
        let garden = &vectors[2].centroid;
        assert!(cv.cosine_similarity(elec) > cv.cosine_similarity(garden));

        // A category with no members yields None.
        assert!(category_vector(&Category::new("plumbing"), &pairs).is_none());
    }
}
