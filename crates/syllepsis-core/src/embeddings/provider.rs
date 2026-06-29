//! The [`EmbeddingProvider`] seam: anything that turns text into an [`Embedding`].
//!
//! The default implementation is [`HashingEmbedder`] (feature hashing, no model, no network).
//! The real provider runs EmbeddingGemma on the same ONNX Runtime stack as the LLM — `ort`
//! natively, `onnxruntime-web` / Transformers.js in the PWA. It becomes a second
//! `impl EmbeddingProvider` and the rest of the app — chunking, note/category vectors, vector
//! search — never changes, because it only ever sees this trait. The hashing embedder is not
//! merely a stub: it is genuinely useful offline (deterministic, instant, dependency free) and is
//! what the tests run against so similarity behavior is reproducible.

use crate::embeddings::vector::Embedding;
use crate::error::CoreResult;
use crate::model::Note;

/// Produces an [`Embedding`] for a piece of text.
///
/// Implementations must be deterministic for a given build (the same text always embeds to the
/// same vector) so that cached vectors stay valid and search results are stable.
pub trait EmbeddingProvider: ProviderInfo {
    /// Vector width this provider emits. All embeddings it returns have this length.
    fn dimensions(&self) -> usize;

    /// Embed a single piece of text as a **document** (the corpus side). Returned vectors are
    /// unit-normalized.
    fn embed(&self, text: &str) -> Embedding;

    /// Embed text as a **search query**. Symmetric models (the default, including the offline
    /// [`HashingEmbedder`](super::HashingEmbedder)) embed queries and documents identically, so
    /// this defaults to [`embed`](Self::embed). An *asymmetric* model wraps a
    /// query in a retrieval instruction the document side never sees — overrides this. The search
    /// engine calls it for the query and [`embed`](Self::embed) for the corpus, so the distinction
    /// lives entirely behind this seam.
    fn embed_query(&self, text: &str) -> Embedding {
        self.embed(text)
    }

    /// Fallible query path used by production search. The legacy infallible methods remain useful
    /// for deterministic test providers, while model failures can now disable only vector ranking.
    fn try_embed_query(&self, text: &str) -> CoreResult<Embedding> {
        Ok(self.embed_query(text))
    }

    /// Generate the two canonical vectors stored for a note: summary and full note.
    fn embed_note_fields(&self, note: &Note) -> CoreResult<(Option<Embedding>, Option<Embedding>)> {
        Ok((self.embed_note_summary(note)?, self.embed_full_note(note)?))
    }

    fn embed_note_summary(&self, note: &Note) -> CoreResult<Option<Embedding>> {
        let summary = (!note.summary.trim().is_empty())
            .then(|| self.embed(&format!("{} {}", note.title, note.summary)));
        Ok(summary)
    }

    fn embed_full_note(&self, note: &Note) -> CoreResult<Option<Embedding>> {
        let content = if note.body.trim().is_empty() {
            &note.summary
        } else {
            &note.body
        };
        let full_text = format!("{} {}", note.title, content);
        let full_note = (!full_text.trim().is_empty()).then(|| self.embed(&full_text));
        Ok(full_note)
    }

    /// Embed several documents. The default loops [`embed`]; a model-backed provider overrides
    /// this to batch on the GPU/accelerator.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Embedding> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// A short, human-readable provider identity for diagnostics and the LLM-management UI.
pub trait ProviderInfo {
    fn name(&self) -> &str;
}
