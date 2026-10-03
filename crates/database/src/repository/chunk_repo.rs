//! Repository for persisting and retrieving content chunks (contentions) using parametric queries

use rusqlite::{params, Connection, OptionalExtension};
use navifs_core::{ByteRange, ChunkId, ChunkType, ContentHash, FileChunk, FileId, LineRange, NaviError, Result};

pub struct ChunkRepository;

impl ChunkRepository {
    /// Inserts or replaces a batch of content chunks inside a transaction with parametric inputs
    pub fn save_batch(conn: &mut Connection, chunks: &[FileChunk]) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| NaviError::Database(format!("Failed to begin chunk transaction: {}", e)))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    r#"
                    INSERT INTO content_chunks (
                        id, file_id, chunk_index, chunk_type, byte_start, byte_end,
                        line_start, line_end, content, token_count, content_hash
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    ON CONFLICT(id) DO UPDATE SET
                        content = excluded.content,
                        token_count = excluded.token_count,
                        content_hash = excluded.content_hash
                    "#,
                )
                .map_err(|e| NaviError::Database(e.to_string()))?;

            for chunk in chunks {
                let chunk_type_str = serde_json::to_string(&chunk.chunk_type)?;
                stmt.execute(params![
                    chunk.id.to_string(),
                    chunk.file_id.to_string(),
                    chunk.chunk_index as i64,
                    chunk_type_str,
                    chunk.byte_range.start as i64,
                    chunk.byte_range.end as i64,
                    chunk.line_range.map(|r| r.start_line as i64),
                    chunk.line_range.map(|r| r.end_line as i64),
                    chunk.content,
                    chunk.token_count as i64,
                    chunk.content_hash.as_str(),
                ])
                .map_err(|e| NaviError::Database(format!("Failed executing chunk insert: {}", e)))?;
            }
        }

        tx.commit()
            .map_err(|e| NaviError::Database(format!("Failed committing chunk transaction: {}", e)))?;

        Ok(())
    }

    /// Fetches a single chunk by ChunkId
    pub fn get_by_id(conn: &Connection, id: &ChunkId) -> Result<Option<FileChunk>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, chunk_index, chunk_type, byte_start, byte_end,
                       line_start, line_end, content, token_count, content_hash
                FROM content_chunks WHERE id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let id_str = id.to_string();
        let chunk = stmt
            .query_row(params![id_str], |row| Self::map_row(row))
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(chunk)
    }

    /// Fetches all content chunks for a specific file, ordered by chunk_index
    pub fn get_for_file(conn: &Connection, file_id: &FileId) -> Result<Vec<FileChunk>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, chunk_index, chunk_type, byte_start, byte_end,
                       line_start, line_end, content, token_count, content_hash
                FROM content_chunks WHERE file_id = ?1 ORDER BY chunk_index ASC
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let file_id_str = file_id.to_string();
        let rows = stmt
            .query_map(params![file_id_str], |row| Self::map_row(row))
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut chunks = Vec::new();
        for chunk_res in rows {
            chunks.push(chunk_res.map_err(|e| NaviError::Database(e.to_string()))?);
        }
        Ok(chunks)
    }

    /// Deletes all chunks associated with a file
    pub fn delete_for_file(conn: &Connection, file_id: &FileId) -> Result<()> {
        let file_id_str = file_id.to_string();
        conn.execute("DELETE FROM content_chunks WHERE file_id = ?1", params![file_id_str])
            .map_err(|e| NaviError::Database(e.to_string()))?;
        Ok(())
    }

    /// Returns total count of content chunks
    pub fn count(conn: &Connection) -> Result<usize> {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM content_chunks", [], |r| r.get(0))
            .map_err(|e| NaviError::Database(e.to_string()))?;
        Ok(count as usize)
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileChunk> {
        let id_str: String = row.get(0)?;
        let file_id_str: String = row.get(1)?;
        let chunk_index: i64 = row.get(2)?;
        let chunk_type_str: String = row.get(3)?;
        let byte_start: i64 = row.get(4)?;
        let byte_end: i64 = row.get(5)?;
        let line_start: Option<i64> = row.get(6)?;
        let line_end: Option<i64> = row.get(7)?;
        let content: String = row.get(8)?;
        let token_count: i64 = row.get(9)?;
        let content_hash_str: String = row.get(10)?;

        let id = ChunkId::parse(&id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let file_id = FileId::parse(&file_id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let chunk_type: ChunkType = serde_json::from_str(&chunk_type_str).map_err(|_| rusqlite::Error::InvalidQuery)?;

        let line_range = match (line_start, line_end) {
            (Some(s), Some(e)) => Some(LineRange::new(s as usize, e as usize)),
            _ => None,
        };

        Ok(FileChunk {
            id,
            file_id,
            chunk_index: chunk_index as u32,
            chunk_type,
            byte_range: ByteRange::new(byte_start as u64, byte_end as u64),
            line_range,
            content,
            token_count: token_count as usize,
            content_hash: ContentHash::new(content_hash_str),
        })
    }
}
