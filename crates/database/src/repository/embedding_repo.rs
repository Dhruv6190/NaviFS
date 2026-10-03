//! Repository for persisting vector embeddings and computing cosine similarity nearest neighbors

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use navifs_core::{ChunkId, EmbeddingId, EmbeddingRecord, FileId, NaviError, Result};

pub struct EmbeddingRepository;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorMatch {
    pub embedding_id: EmbeddingId,
    pub file_id: FileId,
    pub chunk_id: Option<ChunkId>,
    pub similarity: f32,
    pub model_name: String,
}

impl EmbeddingRepository {
    /// Inserts or replaces vector embeddings inside a transaction using parametric inputs
    pub fn save_batch(conn: &mut Connection, records: &[EmbeddingRecord]) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| NaviError::Database(format!("Failed to begin embedding transaction: {}", e)))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    r#"
                    INSERT INTO embeddings (
                        id, file_id, chunk_id, model_name, dimensions, vector, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ON CONFLICT(id) DO UPDATE SET
                        model_name = excluded.model_name,
                        dimensions = excluded.dimensions,
                        vector = excluded.vector
                    "#,
                )
                .map_err(|e| NaviError::Database(e.to_string()))?;

            for record in records {
                let vector_bytes = record.vector_to_bytes();
                stmt.execute(params![
                    record.id.to_string(),
                    record.file_id.to_string(),
                    record.chunk_id.map(|c| c.to_string()),
                    record.model_name,
                    record.dimensions as i64,
                    vector_bytes,
                    record.created_at.to_rfc3339(),
                ])
                .map_err(|e| NaviError::Database(format!("Failed executing embedding insert: {}", e)))?;
            }
        }

        tx.commit()
            .map_err(|e| NaviError::Database(format!("Failed committing embedding transaction: {}", e)))?;

        Ok(())
    }

    /// Fetches the embedding record for a specific chunk
    pub fn get_for_chunk(conn: &Connection, chunk_id: &ChunkId) -> Result<Option<EmbeddingRecord>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, chunk_id, model_name, dimensions, vector, created_at
                FROM embeddings WHERE chunk_id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let chunk_id_str = chunk_id.to_string();
        let record = stmt
            .query_row(params![chunk_id_str], |row| Self::map_row(row))
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(record)
    }

    /// Fetches all embeddings for a specific file
    pub fn get_for_file(conn: &Connection, file_id: &FileId) -> Result<Vec<EmbeddingRecord>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, chunk_id, model_name, dimensions, vector, created_at
                FROM embeddings WHERE file_id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let file_id_str = file_id.to_string();
        let rows = stmt
            .query_map(params![file_id_str], |row| Self::map_row(row))
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut records = Vec::new();
        for rec in rows {
            records.push(rec.map_err(|e| NaviError::Database(e.to_string()))?);
        }
        Ok(records)
    }

    /// Finds top-K nearest neighbors using cosine similarity with parametric model filtering
    pub fn find_nearest_neighbors(
        conn: &Connection,
        query_vector: &[f32],
        model_name: &str,
        limit: usize,
    ) -> Result<Vec<VectorMatch>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, chunk_id, model_name, dimensions, vector, created_at
                FROM embeddings WHERE model_name = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![model_name], |row| Self::map_row(row))
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut matches = Vec::new();
        for rec_res in rows {
            let record = rec_res.map_err(|e| NaviError::Database(e.to_string()))?;
            let similarity = record.cosine_similarity(query_vector);

            matches.push(VectorMatch {
                embedding_id: record.id,
                file_id: record.file_id,
                chunk_id: record.chunk_id,
                similarity,
                model_name: record.model_name,
            });
        }

        // Rank by highest similarity score
        matches.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(limit);

        Ok(matches)
    }

    /// Deletes all embeddings for a file
    pub fn delete_for_file(conn: &Connection, file_id: &FileId) -> Result<()> {
        let file_id_str = file_id.to_string();
        conn.execute("DELETE FROM embeddings WHERE file_id = ?1", params![file_id_str])
            .map_err(|e| NaviError::Database(e.to_string()))?;
        Ok(())
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmbeddingRecord> {
        let id_str: String = row.get(0)?;
        let file_id_str: String = row.get(1)?;
        let chunk_id_str: Option<String> = row.get(2)?;
        let model_name: String = row.get(3)?;
        let dimensions: i64 = row.get(4)?;
        let vector_bytes: Vec<u8> = row.get(5)?;
        let created_str: String = row.get(6)?;

        let id = EmbeddingId::parse(&id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let file_id = FileId::parse(&file_id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let chunk_id = chunk_id_str.and_then(|c| ChunkId::parse(&c).ok());

        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&Utc);

        let vector = EmbeddingRecord::bytes_to_vector(&vector_bytes);

        Ok(EmbeddingRecord {
            id,
            file_id,
            chunk_id,
            model_name,
            dimensions: dimensions as usize,
            vector,
            created_at,
        })
    }
}
