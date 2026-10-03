//! Full-text lexical search repository using SQLite FTS5 with BM25 ranking and snippet generation

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use navifs_core::{ChunkId, FileId, NaviError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtsSearchResult {
    pub chunk_id: Option<ChunkId>,
    pub file_id: FileId,
    pub filename: String,
    pub path: String,
    pub snippet: String,
    pub score: f32,
}

pub struct FtsRepository;

impl FtsRepository {
    /// Sanitizes raw user input into safe FTS5 query tokens with prefix matching
    pub fn sanitize_query(query: &str) -> String {
        let tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|s| !s.is_empty())
            .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
            .collect();

        if tokens.is_empty() {
            "\"\"".to_string()
        } else {
            tokens.join(" AND ")
        }
    }

    /// Performs full-text search across filenames, paths, and extracted content using parametric FTS5 MATCH
    pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let sanitized = Self::sanitize_query(query);
        if sanitized == "\"\"" {
            return Ok(Vec::new());
        }

        let mut stmt = conn
            .prepare(
                r#"
                SELECT
                    chunk_id,
                    file_id,
                    filename,
                    path,
                    snippet(fts_content, 4, '<mark>', '</mark>', '...', 32) AS snippet_text,
                    bm25(fts_content, 2.0, 5.0, 1.0) AS score
                FROM fts_content
                WHERE fts_content MATCH ?1
                ORDER BY score ASC
                LIMIT ?2
                "#,
            )
            .map_err(|e| NaviError::SearchError(format!("Failed to prepare FTS5 query: {}", e)))?;

        let rows = stmt
            .query_map(params![sanitized, limit as i64], |row| {
                let chunk_id_opt: Option<String> = row.get(0)?;
                let file_id_str: String = row.get(1)?;
                let filename: String = row.get(2)?;
                let path: String = row.get(3)?;
                let snippet: String = row.get(4)?;
                let raw_score: f64 = row.get(5)?;

                let file_id = FileId::parse(&file_id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let chunk_id = chunk_id_opt.and_then(|c| ChunkId::parse(&c).ok());

                // SQLite BM25 returns lower negative values for higher relevance
                let score = (-raw_score as f32).max(0.01);

                Ok(FtsSearchResult {
                    chunk_id,
                    file_id,
                    filename,
                    path,
                    snippet,
                    score,
                })
            })
            .map_err(|e| NaviError::SearchError(format!("Failed executing FTS5 search: {}", e)))?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r.map_err(|e| NaviError::SearchError(e.to_string()))?);
        }
        Ok(results)
    }

    /// Searches specifically by file name or path using column-scoped parametric queries
    pub fn search_by_filename_or_path(conn: &Connection, term: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let clean_term = term.replace('"', "\"\"");
        let scoped_query = format!("filename: \"{}\"* OR path: \"{}\"*", clean_term, clean_term);

        let mut stmt = conn
            .prepare(
                r#"
                SELECT
                    chunk_id,
                    file_id,
                    filename,
                    path,
                    snippet(fts_content, 4, '<mark>', '</mark>', '...', 20) AS snippet_text,
                    bm25(fts_content, 5.0, 5.0, 0.5) AS score
                FROM fts_content
                WHERE fts_content MATCH ?1
                ORDER BY score ASC
                LIMIT ?2
                "#,
            )
            .map_err(|e| NaviError::SearchError(format!("Failed to prepare path FTS query: {}", e)))?;

        let rows = stmt
            .query_map(params![scoped_query, limit as i64], |row| {
                let chunk_id_opt: Option<String> = row.get(0)?;
                let file_id_str: String = row.get(1)?;
                let filename: String = row.get(2)?;
                let path: String = row.get(3)?;
                let snippet: String = row.get(4)?;
                let raw_score: f64 = row.get(5)?;

                let file_id = FileId::parse(&file_id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let chunk_id = chunk_id_opt.and_then(|c| ChunkId::parse(&c).ok());
                let score = (-raw_score as f32).max(0.01);

                Ok(FtsSearchResult {
                    chunk_id,
                    file_id,
                    filename,
                    path,
                    snippet,
                    score,
                })
            })
            .map_err(|e| NaviError::SearchError(format!("Failed executing path FTS search: {}", e)))?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r.map_err(|e| NaviError::SearchError(e.to_string()))?);
        }
        Ok(results)
    }

    /// Manually triggers FTS5 index rebuild
    pub fn rebuild_index(conn: &Connection) -> Result<()> {
        conn.execute("INSERT INTO fts_content(fts_content) VALUES('rebuild')", [])
            .map_err(|e| NaviError::SearchError(format!("Failed to rebuild FTS5 index: {}", e)))?;
        Ok(())
    }
}
