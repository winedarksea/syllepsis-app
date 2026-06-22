//! Persistent `_derived/search.sqlite` index: FTS5 rows plus stored note/chunk vectors.
//!
//! The in-memory [`SearchEngine`](super::SearchEngine) remains the source of ranking semantics.
//! This layer persists its snapshot into SQLite so Phase 2 has a real derived index on disk; queries
//! read exact/FTS/vector candidates back from that index, then reuse the same RRF/result shaping.

use rusqlite::{params, Connection};
use std::path::Path;

use crate::embeddings::Embedding;
use crate::error::{CoreError, CoreResult};
use crate::search::exact::match_exact;
use crate::search::results::{FacetCount, SearchHit, SearchResults};
use crate::search::rrf::reciprocal_rank_fusion;
use crate::search::SearchEngine;

const SQLITE_SEARCH_DB: &str = "search.sqlite";
const SCHEMA_VERSION: i64 = 1;

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
                    "INSERT INTO note_vectors(note_index, part_index, dim, vector_json)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        idx as i64,
                        part_idx as i64,
                        vector.len() as i64,
                        serde_json::to_string(&vector.0)?,
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn search(
        &self,
        engine: &SearchEngine,
        query: &str,
        category_filter: &[String],
    ) -> CoreResult<SearchResults> {
        let exact = ids_only(match_exact(engine.documents(), query));
        let fts = self.fts_ranked_indices(query)?;
        let vector = self.vector_ranked_indices(&engine.query_embedding(query))?;
        let fused = reciprocal_rank_fusion(&[exact, fts, vector], engine.search_config());
        let facets = self.facet_counts(fused.iter().map(|(idx, _)| *idx))?;
        let hits: Vec<SearchHit> = fused
            .into_iter()
            .filter(|(idx, _)| engine.passes_category_filter(*idx, category_filter))
            .take(engine.search_config().result_limit)
            .map(|(idx, score)| engine.search_hit_for_index(idx, score, query))
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
                vector_json TEXT NOT NULL,
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
            "INSERT OR REPLACE INTO search_meta(key, value) VALUES ('vector_backend', 'sqlite-json-f32')",
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

    fn vector_ranked_indices(&self, query: &Embedding) -> CoreResult<Vec<usize>> {
        if query.magnitude() <= f32::EPSILON {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT note_index, vector_json FROM note_vectors ORDER BY note_index")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)? as usize, row.get::<_, String>(1)?))
        })?;
        let mut best_by_note: Vec<(usize, f32)> = Vec::new();
        for row in rows {
            let (note_index, vector_json) = row?;
            let values: Vec<f32> = serde_json::from_str(&vector_json)?;
            let score = Embedding::new(values).cosine_similarity(query);
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
        Ok(best_by_note.into_iter().map(|(idx, _)| idx).collect())
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

fn ids_only(ranked: Vec<(usize, f32)>) -> Vec<usize> {
    ranked.into_iter().map(|(idx, _)| idx).collect()
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

    fn note(title: &str, body: &str, cats: &[&str]) -> Note {
        let mut note = Note::new(ObjectType::Note, title, "syllepsis_001");
        note.body = body.to_string();
        note.categories = cats.iter().map(|c| c.to_string()).collect();
        note
    }

    fn engine() -> SearchEngine {
        let config = Config::default();
        SearchEngine::build(
            vec![
                note("Kitchen wiring", "breaker panel outlets", &["electrical"]),
                note("Garden", "compost roses soil", &["garden"]),
            ],
            Box::new(HashingEmbedder::new(config.embedding.dimensions)),
            &config,
        )
    }

    #[test]
    fn rebuild_persists_notes_vectors_and_fts_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = SqliteSearchIndex::open(dir.path()).unwrap();
        let engine = engine();
        index.rebuild_from_engine(&engine).unwrap();

        assert_eq!(index.persisted_note_count().unwrap(), 2);
        assert_eq!(index.vector_backend().unwrap(), "sqlite-json-f32");
        let results = index.search(&engine, "breaker panel", &[]).unwrap();
        assert_eq!(results.hits[0].title, "Kitchen wiring");
        assert!(results.facets.iter().any(|f| f.category == "electrical"));
        assert!(dir.path().join(SQLITE_SEARCH_DB).exists());
    }
}
