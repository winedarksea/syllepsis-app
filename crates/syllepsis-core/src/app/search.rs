//! Application command surface for retrieval (search, related notes, embedding diagnostics).
//!
//! Like the rest of [`crate::app`], these are framework-agnostic functions over a [`Book`] that
//! the Tauri shell wraps as commands. Each builds a [`SearchEngine`] from the book's current
//! notes and the default local embedder. Search persists the current snapshot into `_derived`
//! SQLite/FTS5 storage before querying; related/diagnostics reuse the same engine
//! snapshot directly.

use serde::{Deserialize, Serialize};

use crate::embeddings::{category_vector, load_embedding_corpus, try_select_embedder, Embedding, EmbeddingCoverage, NoteVectors};
use crate::error::CoreResult;
use crate::model::Note;
use crate::search::{
    EmbeddingDiagnostics, RelatedNote, SearchEngine, SearchResults, SqliteSearchIndex,
};
use crate::storage::layout;
use crate::storage::{Book, NoteStore};

/// Build a search engine over the book's *visible* notes (hidden — archived/private — and
/// pending-deletion notes are excluded so they never surface in RAG results), using the
/// configured embedding provider.
fn engine_for(book: &Book) -> CoreResult<(SearchEngine, EmbeddingCoverage)> {
    let mut notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| n.metadata.is_visible_in_default_views())
        .collect();
    notes.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let loaded = load_embedding_corpus(book, &notes)?;
    Ok((
        SearchEngine::build_from_vectors(notes, loaded.vectors, &book.config),
        loaded.coverage,
    ))
}

/// Full search: exact + BM25 + vector fused with RRF, optionally narrowed to `category_filter`.
pub fn search(book: &Book, query: &str, category_filter: &[String]) -> CoreResult<SearchResults> {
    let query_embedding = try_select_embedder(book.models_root(), &book.config.embedding)
        .and_then(|provider| provider.try_embed_query(query))
        .ok();
    search_with_query_embedding(book, query, category_filter, query_embedding.as_ref())
}

pub fn search_with_query_embedding(
    book: &Book,
    query: &str,
    category_filter: &[String],
    query_embedding: Option<&Embedding>,
) -> CoreResult<SearchResults> {
    let started = std::time::Instant::now();
    let (engine, _) = engine_for(book)?;
    let mut index = SqliteSearchIndex::open(&layout::derived_dir(&book.root))?;
    index.rebuild_from_engine(&engine)?;
    let results = index.search(&engine, query, category_filter, query_embedding)?;
    tracing::info!(
        query = query,
        filters = category_filter.len(),
        hits = results.hits.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "search: query complete"
    );
    Ok(results)
}

/// Notes related to `id` for the related carousel (vector neighbors, category-upweighted).
pub fn related_notes(book: &Book, id: &str) -> CoreResult<Vec<RelatedNote>> {
    Ok(engine_for(book)?.0.related(id))
}

/// Embedding health report: near-duplicates and blind-spot (weakly connected) notes.
pub fn embedding_diagnostics(book: &Book) -> CoreResult<EmbeddingDiagnostics> {
    Ok(engine_for(book)?.0.diagnostics())
}

pub fn embedding_coverage(book: &Book) -> CoreResult<EmbeddingCoverage> {
    Ok(engine_for(book)?.1)
}

/// Embedding coverage for a single category.
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryEmbeddingStats {
    pub total_notes: usize,
    pub embedded_notes: usize,
    pub has_vector: bool,
}

pub fn category_embedding_stats(book: &Book, name: &str) -> CoreResult<CategoryEmbeddingStats> {
    let notes: Vec<Note> = book
        .store
        .read_all_notes()?
        .into_iter()
        .filter(|n| n.metadata.is_visible_in_default_views() && n.categories.iter().any(|c| c == name))
        .collect();

    let corpus = load_embedding_corpus(book, &notes)?;

    let embedded_notes = corpus
        .vectors
        .iter()
        .filter(|v| v.centroid.magnitude() > f32::EPSILON)
        .count();

    let cat = book.store.categories()?.into_iter().find(|c| c.name == name);
    let has_vector = if let Some(cat) = cat {
        let pairs: Vec<(&Note, &NoteVectors)> =
            notes.iter().zip(corpus.vectors.iter()).collect();
        category_vector(&cat, &pairs).is_some()
    } else {
        false
    };

    Ok(CategoryEmbeddingStats {
        total_notes: notes.len(),
        embedded_notes,
        has_vector,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{create_note, update_note};
    use crate::model::ObjectType;
    use crate::storage::Book;

    fn book() -> (tempfile::TempDir, Book) {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path(), "Test").unwrap();
        (dir, book)
    }

    fn add(book: &Book, title: &str, body: &str, cats: &[&str]) {
        let mut n = create_note(book, ObjectType::Note, title, None).unwrap();
        n.body = body.to_string();
        n.categories = cats.iter().map(|c| c.to_string()).collect();
        update_note(book, n).unwrap();
    }

    #[test]
    fn search_finds_and_facets_notes() {
        let (_d, book) = book();
        add(
            &book,
            "Kitchen",
            "breaker panel and outlets",
            &["electrical"],
        );
        add(&book, "Garden", "roses and compost", &["garden"]);

        let results = search(&book, "breaker panel", &[]).unwrap();
        assert_eq!(results.hits[0].title, "Kitchen");
        assert!(results.facets.iter().any(|f| f.category == "electrical"));
        assert!(layout::derived_dir(&book.root)
            .join("search.sqlite")
            .exists());
    }

    #[test]
    fn archived_notes_are_not_searchable() {
        let (_d, book) = book();
        let mut n = create_note(&book, ObjectType::Note, "Hidden", None).unwrap();
        n.body = "breaker panel secret".into();
        n.metadata.lifecycle.archived = true;
        update_note(&book, n).unwrap();

        assert!(search(&book, "breaker panel", &[]).unwrap().hits.is_empty());
    }

    #[test]
    fn related_returns_neighbors() {
        let (_d, book) = book();
        add(
            &book,
            "Compost A",
            "compost soil greens browns garden",
            &["garden"],
        );
        add(
            &book,
            "Compost B",
            "garden compost pile soil watering",
            &["garden"],
        );
        add(&book, "Wiring", "electrical breaker panel", &["electrical"]);
        let notes = book.store.read_all_notes().unwrap();
        crate::embeddings::repository::write_test_sidecars(&book, &notes);

        // Resolve the id of Compost A.
        let hits = search(&book, "Compost A", &[]).unwrap();
        let id = &hits.hits[0].note_id;
        let related = related_notes(&book, id).unwrap();
        assert!(related.iter().any(|r| r.title == "Compost B"));
    }

    #[test]
    fn repeated_consumers_do_not_rewrite_or_recompute_note_embeddings() {
        let (_d, book) = book();
        add(&book, "Compost A", "compost soil garden", &["garden"]);
        add(&book, "Compost B", "garden compost watering", &["garden"]);
        let notes = book.store.read_all_notes().unwrap();
        crate::embeddings::repository::write_test_sidecars(&book, &notes);
        let paths = notes
            .iter()
            .map(|note| layout::embedding_sidecar_path(&book.root, &note.id))
            .collect::<Vec<_>>();
        let before = paths
            .iter()
            .map(std::fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let first = search(&book, "compost", &[]).unwrap();
        let id = first.hits[0].note_id.clone();
        let _ = search(&book, "compost", &[]).unwrap();
        let _ = related_notes(&book, &id).unwrap();
        let _ = embedding_diagnostics(&book).unwrap();

        let after = paths
            .iter()
            .map(std::fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(before, after);
    }
}
