//! Persistent `_derived/search.sqlite` index: FTS5 rows plus stored note/chunk vectors.
//!
//! The in-memory [`SearchEngine`](super::SearchEngine) remains the source of ranking semantics.
//! This layer persists its snapshot into SQLite so Phase 2 has a real derived index on disk; queries
//! read exact/FTS/vector candidates back from that index, then reuse the same RRF/result shaping.

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

use crate::embeddings::Embedding;
use crate::error::{CoreError, CoreResult};
use crate::search::exact::match_exact;
use crate::search::filter::SearchFilter;
use crate::search::results::{FacetCount, SearchHit, SearchRankingSignals, SearchResults};
use crate::search::rrf::reciprocal_rank_fusion;
use crate::search::SearchEngine;

const SQLITE_SEARCH_DB: &str = "search.sqlite";
const SCHEMA_VERSION: i64 = 2;

pub struct SqliteSearchIndex {
    conn: Connection,
}

impl SqliteSearchIndex {
    pub fn open(derived_dir: &Path) -> CoreResult<SqliteSearchIndex> {
        std::fs::create_dir_all(derived_dir)?;
        let conn = Connection::open(derived_dir.join(SQLITE_SEARCH_DB))?;
        let index = SqliteSearchIndex { conn };
        index.ensure_schema()?;
        Ok(index)
    }

    pub fn rebuild_from_engine(&mut self, engine: &SearchEngine) -> CoreResult<()> {
        let snapshot_hash = engine_snapshot_hash(engine);
        let existing_hash = self
            .conn
            .query_row(
                "SELECT value FROM search_meta WHERE key = 'snapshot_hash'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if existing_hash.as_deref() == Some(snapshot_hash.as_str()) {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "
            DELETE FROM note_vectors;
            DELETE FROM note_categories;
            DELETE FROM notes;
            DELETE FROM note_fts;
            ",
        )?;

        for (idx, note) in engine.notes().iter().enumerate() {
            tx.execute(
                "INSERT INTO notes(note_index, note_id, title, summary, snippet_source, document)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    idx as i64,
                    note.id.to_string(),
                    note.title,
                    note.summary,
                    note.body,
                    engine.documents()[idx],
                ],
            )?;
            tx.execute(
                "INSERT INTO note_fts(rowid, title, summary, document) VALUES (?1, ?2, ?3, ?4)",
                params![
                    idx as i64 + 1,
                    note.title,
                    note.summary,
                    engine.documents()[idx],
                ],
            )?;
            for category in &note.categories {
                tx.execute(
                    "INSERT INTO note_categories(note_index, category) VALUES (?1, ?2)",
                    params![idx as i64, category],
                )?;
            }
            for (part_idx, vector) in engine.vectors()[idx].parts.iter().enumerate() {
                tx.execute(
                    "INSERT INTO note_vectors(note_index, part_index, dim, vector_blob)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        idx as i64,
                        part_idx as i64,
                        vector.len() as i64,
                        embedding_to_blob(vector),
                    ],
                )?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO search_meta(key, value) VALUES ('snapshot_hash', ?1)",
            params![snapshot_hash],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn search(
        &self,
        engine: &SearchEngine,
        query: &str,
        filter: &SearchFilter,
        query_embedding: Option<&Embedding>,
    ) -> CoreResult<SearchResults> {
        let exact = ids_only(match_exact(engine.documents(), query));
        let fts = self.fts_ranked_indices(query)?;
        let vector_scored = match query_embedding {
            Some(emb) => self.vector_ranked_indices(emb)?,
            None => Vec::new(),
        };
        let vector: Vec<usize> = vector_scored.iter().map(|(i, _)| *i).collect();
        let vector_cosines: HashMap<usize, f32> = vector_scored.into_iter().collect();

        let exact_ranks = rank_map(&exact);
        let fts_ranks = rank_map(&fts);
        let vector_ranks = rank_map(&vector);

        let mut fused = reciprocal_rank_fusion(&[exact, fts, vector], engine.search_config());

        // Tiebreaker: among equal RRF scores, longer-body notes rank first (penalizes title-only
        // notes — body length 0 — without touching the zero-centroid path).
        fused.sort_by(|(a_idx, a_score), (b_idx, b_score)| {
            b_score
                .total_cmp(a_score)
                .then_with(|| {
                    engine.notes()[*b_idx]
                        .body
                        .len()
                        .cmp(&engine.notes()[*a_idx].body.len())
                })
        });

        let facets = self.facet_counts(fused.iter().map(|(idx, _)| *idx))?;
        let hits: Vec<SearchHit> = fused
            .into_iter()
            .filter(|(idx, _)| engine.passes_filter(*idx, filter))
            .take(engine.search_config().result_limit)
            .map(|(idx, score)| {
                let signals = ranking_signals(
                    idx,
                    score,
                    &exact_ranks,
                    &fts_ranks,
                    &vector_ranks,
                    engine.search_config().rrf_k,
                    &vector_cosines,
                );
                engine.search_hit_for_index(idx, score, query, signals)
            })
            .collect();
        Ok(SearchResults { hits, facets })
    }

    pub fn persisted_note_count(&self) -> CoreResult<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn vector_backend(&self) -> CoreResult<String> {
        self.conn
            .query_row(
                "SELECT value FROM search_meta WHERE key = 'vector_backend'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn ensure_schema(&self) -> CoreResult<()> {
        let existing_version = self
            .conn
            .query_row(
                "SELECT value FROM search_meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|value| value.parse::<i64>().ok());
        if existing_version != Some(SCHEMA_VERSION) {
            self.conn.execute_batch(
                "
                DROP TABLE IF EXISTS note_vectors;
                DROP TABLE IF EXISTS note_categories;
                DROP TABLE IF EXISTS notes;
                DROP TABLE IF EXISTS note_fts;
                DROP TABLE IF EXISTS search_meta;
                ",
            )?;
        }
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS search_meta(
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS notes(
                note_index INTEGER PRIMARY KEY,
                note_id TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                snippet_source TEXT NOT NULL,
                document TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS note_categories(
                note_index INTEGER NOT NULL REFERENCES notes(note_index) ON DELETE CASCADE,
                category TEXT NOT NULL,
                PRIMARY KEY(note_index, category)
            );
            CREATE TABLE IF NOT EXISTS note_vectors(
                note_index INTEGER NOT NULL REFERENCES notes(note_index) ON DELETE CASCADE,
                part_index INTEGER NOT NULL,
                dim INTEGER NOT NULL,
                vector_blob BLOB NOT NULL,
                PRIMARY KEY(note_index, part_index)
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS note_fts
                USING fts5(title, summary, document, tokenize = 'unicode61');
            ",
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO search_meta(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO search_meta(key, value) VALUES ('vector_backend', 'sqlite-blob-f32')",
            [],
        )?;
        Ok(())
    }

    fn fts_ranked_indices(&self, query: &str) -> CoreResult<Vec<usize>> {
        let fts_query = fts_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT rowid - 1 AS note_index
             FROM note_fts
             WHERE note_fts MATCH ?1
             ORDER BY bm25(note_fts)",
        )?;
        let rows = stmt.query_map(params![fts_query], |row| row.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row? as usize);
        }
        Ok(out)
    }

    /// Returns `(note_index, best_cosine_similarity)` pairs, sorted descending by cosine.
    /// The cosine scores are threaded through to `SearchRankingSignals::vector_similarity`.
    fn vector_ranked_indices(&self, query: &Embedding) -> CoreResult<Vec<(usize, f32)>> {
        if query.magnitude() <= f32::EPSILON {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT note_index, vector_blob FROM note_vectors ORDER BY note_index")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)? as usize, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut best_by_note: Vec<(usize, f32)> = Vec::new();
        for row in rows {
            let (note_index, vector_blob) = row?;
            let score = embedding_from_blob(&vector_blob)?.cosine_similarity(query);
            if score <= 0.0 {
                continue;
            }
            match best_by_note.iter_mut().find(|(idx, _)| *idx == note_index) {
                Some((_, best)) if score > *best => *best = score,
                Some(_) => {}
                None => best_by_note.push((note_index, score)),
            }
        }
        best_by_note.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(best_by_note)
    }

    fn facet_counts(&self, indices: impl Iterator<Item = usize>) -> CoreResult<Vec<FacetCount>> {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        let mut stmt = self
            .conn
            .prepare("SELECT category FROM note_categories WHERE note_index = ?1")?;
        for idx in indices {
            let rows = stmt.query_map(params![idx as i64], |row| row.get::<_, String>(0))?;
            for row in rows {
                *counts.entry(row?).or_insert(0) += 1;
            }
        }
        let mut facets: Vec<FacetCount> = counts
            .into_iter()
            .map(|(category, count)| FacetCount { category, count })
            .collect();
        facets.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.category.cmp(&b.category))
        });
        Ok(facets)
    }
}

fn engine_snapshot_hash(engine: &SearchEngine) -> String {
    let mut hasher = Sha256::new();
    for (index, note) in engine.notes().iter().enumerate() {
        hasher.update(note.id.as_str().as_bytes());
        hasher.update(engine.documents()[index].as_bytes());
        for category in &note.categories {
            hasher.update((category.len() as u64).to_le_bytes());
            hasher.update(category.as_bytes());
        }
        for vector in &engine.vectors()[index].parts {
            hasher.update((vector.len() as u64).to_le_bytes());
            for value in &vector.0 {
                hasher.update(value.to_le_bytes());
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn embedding_to_blob(embedding: &Embedding) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for value in &embedding.0 {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn embedding_from_blob(bytes: &[u8]) -> CoreResult<Embedding> {
    if !bytes.len().is_multiple_of(4) {
        return Err(CoreError::parse(
            "SQLite embedding",
            "BLOB length is not divisible by four",
        ));
    }
    Ok(Embedding::new(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
            .collect(),
    ))
}

fn ids_only(ranked: Vec<(usize, f32)>) -> Vec<usize> {
    ranked.into_iter().map(|(idx, _)| idx).collect()
}

fn rank_map(indices: &[usize]) -> HashMap<usize, usize> {
    indices
        .iter()
        .enumerate()
        .map(|(rank, idx)| (*idx, rank))
        .collect()
}

fn ranking_signals(
    note_index: usize,
    total: f32,
    exact_ranks: &HashMap<usize, usize>,
    bm25_ranks: &HashMap<usize, usize>,
    vector_ranks: &HashMap<usize, usize>,
    rrf_k: f32,
    vector_cosines: &HashMap<usize, f32>,
) -> SearchRankingSignals {
    let exact = exact_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let bm25 = bm25_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let vector = vector_ranks
        .get(&note_index)
        .map(|rank| 1.0 / (rrf_k + *rank as f32 + 1.0))
        .unwrap_or(0.0);
    let vector_similarity = vector_cosines.get(&note_index).copied().unwrap_or(0.0);
    SearchRankingSignals {
        exact,
        bm25,
        vector,
        total,
        vector_similarity,
    }
}

fn fts_query(query: &str) -> String {
    crate::text::tokenize(query).join(" OR ")
}

impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        CoreError::Model(format!("sqlite search index: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::embeddings::HashingEmbedder;
    use crate::model::{Note, ObjectType};
    use crate::search::filter::SearchFilter;

    fn note(title: &str, body: &str, cats: &[&str]) -> Note {
        let mut note = Note::new(ObjectType::Note, title, "syllepsis_001");
        note.body = body.to_string();
        note.categories = cats.iter().map(|c| c.to_string()).collect();
        note
    }

    fn engine_from(notes: Vec<Note>) -> SearchEngine {
        let config = Config::default();
        SearchEngine::build(
            notes,
            Box::new(HashingEmbedder::new(config.embedding.dimensions)),
            &config,
        )
    }

    fn engine() -> SearchEngine {
        engine_from(vec![
            note("Kitchen wiring", "breaker panel outlets", &["electrical"]),
            note("Garden", "compost roses soil", &["garden"]),
        ])
    }

    fn open_index(engine: &mut SearchEngine) -> (tempfile::TempDir, SqliteSearchIndex) {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteSearchIndex::open(dir.path()).unwrap();
        index.rebuild_from_engine(engine).unwrap();
        (dir, index)
    }

    #[test]
    fn rebuild_persists_notes_vectors_and_fts_results() {
        let mut eng = engine();
        let (dir, index) = open_index(&mut eng);

        assert_eq!(index.persisted_note_count().unwrap(), 2);
        assert_eq!(index.vector_backend().unwrap(), "sqlite-blob-f32");
        let query_emb = eng.query_embedding("breaker panel");
        let results = index
            .search(&eng, "breaker panel", &SearchFilter::default(), Some(&query_emb))
            .unwrap();
        assert_eq!(results.hits[0].title, "Kitchen wiring");
        assert!(results.facets.iter().any(|f| f.category == "electrical"));
        assert!(dir.path().join(SQLITE_SEARCH_DB).exists());
    }

    #[test]
    fn vector_similarity_is_populated() {
        let mut eng = engine();
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker panel");
        let results = index
            .search(&eng, "breaker panel", &SearchFilter::default(), Some(&query_emb))
            .unwrap();
        assert!(!results.hits.is_empty());
        // The top hit should have a non-zero vector_similarity when a matching embedding exists.
        assert!(
            results.hits[0].ranking_signals.vector_similarity > 0.0,
            "expected non-zero cosine similarity for a matching note"
        );
    }

    #[test]
    fn length_filter_excludes_title_only_note() {
        // The tiebreaker only applies when RRF scores are equal; a title-only note can still
        // legitimately outscore a full note on exact/BM25 for a query matching its title exactly.
        // The correct user-facing lever is the length filter.
        let full = note("Breaker panel wiring", "Install the breaker panel and run the outlets to each room. Use 20-amp breakers.", &["electrical"]);
        let title_only = note("Breaker panel", "", &["electrical"]);
        let mut eng = engine_from(vec![full, title_only]);
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker panel");

        // Without a length filter both appear.
        let all = index
            .search(&eng, "breaker panel", &SearchFilter::default(), Some(&query_emb))
            .unwrap();
        assert_eq!(all.hits.len(), 2);

        // With min_body_len the title-only note is filtered out.
        let filter = SearchFilter {
            min_body_len: Some(10),
            ..Default::default()
        };
        let filtered = index
            .search(&eng, "breaker panel", &filter, Some(&query_emb))
            .unwrap();
        assert_eq!(filtered.hits.len(), 1);
        assert_eq!(filtered.hits[0].title, "Breaker panel wiring");
    }

    #[test]
    fn updated_after_filter_narrows_hits() {
        use chrono::Utc;
        let mut notes = vec![
            note("Old note", "old content about breaker", &["electrical"]),
            note("New note", "new content about breaker", &["electrical"]),
        ];
        // Manually push the "New note" updated timestamp into the future.
        let future = Utc::now() + chrono::Duration::hours(1);
        notes[1].metadata.dates.updated = future;

        let mut eng = engine_from(notes);
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker");

        // Filter: only notes updated after "now" — should keep only the future-dated one.
        let filter = SearchFilter {
            updated_after: Some(Utc::now()),
            ..Default::default()
        };
        let results = index
            .search(&eng, "breaker", &filter, Some(&query_emb))
            .unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].title, "New note");
    }

    #[test]
    fn starred_only_filter_narrows_hits() {
        let mut notes = vec![
            note("Regular note", "content about breaker", &["electrical"]),
            note("Starred note", "more content about breaker", &["electrical"]),
        ];
        notes[1].metadata.classification.starred = true;

        let mut eng = engine_from(notes);
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker");

        let filter = SearchFilter {
            starred_only: true,
            ..Default::default()
        };
        let results = index
            .search(&eng, "breaker", &filter, Some(&query_emb))
            .unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].title, "Starred note");
        assert!(results.hits[0].starred);
    }

    #[test]
    fn object_type_filter_narrows_hits() {
        let mut notes = vec![
            note("A note", "content about breaker", &["electrical"]),
        ];
        let mut todo = Note::new(ObjectType::Todo, "A todo", "syllepsis_001");
        todo.body = "todo content about breaker".into();
        todo.categories = vec!["electrical".into()];
        notes.push(todo);

        let mut eng = engine_from(notes);
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker");

        let filter = SearchFilter {
            object_types: vec![ObjectType::Todo],
            ..Default::default()
        };
        let results = index
            .search(&eng, "breaker", &filter, Some(&query_emb))
            .unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].object_type, ObjectType::Todo);
    }

    #[test]
    fn length_filter_narrows_hits() {
        let notes = vec![
            note("Short note", "breaker", &["electrical"]),
            note("Long note", &"breaker panel wiring outlet ".repeat(20), &["electrical"]),
        ];
        let mut eng = engine_from(notes);
        let (_dir, index) = open_index(&mut eng);
        let query_emb = eng.query_embedding("breaker");

        let filter = SearchFilter {
            min_body_len: Some(50),
            ..Default::default()
        };
        let results = index
            .search(&eng, "breaker", &filter, Some(&query_emb))
            .unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].title, "Long note");
    }

    #[test]
    fn facets_remain_full_under_active_filter() {
        // Both notes must match the query so they both appear in the fused (unfiltered) set.
        // A category filter then narrows hits but facets should still include the filtered category.
        let notes = vec![
            note("Electrical note", "the breaker panel has outlets", &["electrical"]),
            note("Garden note", "the garden has compost and roses", &["garden"]),
        ];
        let mut eng = engine_from(notes);
        let (_dir, index) = open_index(&mut eng);

        // "the" appears in both note bodies → both land in the fused set.
        let filter = SearchFilter {
            categories: vec!["electrical".into()],
            ..Default::default()
        };
        let results = index
            .search(&eng, "the", &filter, None)
            .unwrap();
        // Filtered hits contain only electrical.
        assert!(results.hits.iter().all(|h| h.categories.contains(&"electrical".into())));
        // But facets are from the unfiltered set, so both categories appear.
        assert!(
            results.facets.iter().any(|f| f.category == "garden"),
            "garden facet should appear even when filtered to electrical"
        );
        assert!(results.facets.iter().any(|f| f.category == "electrical"));
    }

    #[test]
    fn schema_v1_is_rebuilt_for_blob_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SQLITE_SEARCH_DB);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE search_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                INSERT INTO search_meta VALUES ('schema_version', '1');
                CREATE TABLE note_vectors(
                    note_index INTEGER,
                    part_index INTEGER,
                    dim INTEGER,
                    vector_json TEXT
                );
                ",
            )
            .unwrap();
        drop(connection);

        let index = SqliteSearchIndex::open(dir.path()).unwrap();
        assert_eq!(index.vector_backend().unwrap(), "sqlite-blob-f32");
        let column: String = index
            .conn
            .query_row(
                "SELECT name FROM pragma_table_info('note_vectors') WHERE name = 'vector_blob'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(column, "vector_blob");
    }
}
