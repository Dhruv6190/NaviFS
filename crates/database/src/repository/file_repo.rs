//! Repository for persisting and retrieving file records using strictly parametric queries

use chrono::{DateTime, Utc};
use navifs_core::{
    ContentHash, FileId, FileIdentity, FileStatus, MimeType, NaviError, PathFingerprint, Result,
};
use rusqlite::{params, Connection, OptionalExtension};

pub struct FileRepository;

impl FileRepository {
    /// Upserts a file record into the database using parametric inputs
    pub fn upsert(conn: &Connection, file: &FileIdentity) -> Result<()> {
        let status_json = serde_json::to_string(&file.status)?;
        let hash_str = file.content_hash.as_ref().map(|h| h.as_str());

        conn.execute(
            r#"
            INSERT INTO files (
                id, path, filename, extension, mime_type, size_bytes,
                content_hash, status, created_at, modified_at, indexed_at, file_index, device_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                filename = excluded.filename,
                extension = excluded.extension,
                mime_type = excluded.mime_type,
                size_bytes = excluded.size_bytes,
                content_hash = excluded.content_hash,
                status = excluded.status,
                modified_at = excluded.modified_at,
                indexed_at = excluded.indexed_at,
                file_index = excluded.file_index,
                device_id = excluded.device_id
            "#,
            params![
                file.id.to_string(),
                file.fingerprint.as_str(),
                file.fingerprint.filename(),
                file.fingerprint.extension(),
                file.mime_type.as_str(),
                file.size_bytes as i64,
                hash_str,
                status_json,
                file.created_at.to_rfc3339(),
                file.modified_at.to_rfc3339(),
                file.indexed_at.map(|d| d.to_rfc3339()),
                file.file_index.map(|i| i as i64),
                file.device_id.map(|d| d as i64),
            ],
        )
        .map_err(|e| NaviError::Database(format!("FileRepository::upsert failed: {}", e)))?;

        Ok(())
    }

    /// Fetches a file by its unique FileId
    pub fn get_by_id(conn: &Connection, id: &FileId) -> Result<Option<FileIdentity>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, path, mime_type, size_bytes, content_hash, status,
                       created_at, modified_at, indexed_at, file_index, device_id
                FROM files WHERE id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let id_str = id.to_string();
        let result = stmt
            .query_row(params![id_str], Self::map_row)
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(result)
    }

    /// Fetches a file by its normalized path
    pub fn get_by_path(conn: &Connection, path: &str) -> Result<Option<FileIdentity>> {
        let normalized = path.replace('\\', "/");
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, path, mime_type, size_bytes, content_hash, status,
                       created_at, modified_at, indexed_at, file_index, device_id
                FROM files WHERE path = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let result = stmt
            .query_row(params![normalized], Self::map_row)
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(result)
    }

    /// Fetches a file by its content hash
    pub fn get_by_hash(conn: &Connection, hash: &ContentHash) -> Result<Option<FileIdentity>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, path, mime_type, size_bytes, content_hash, status,
                       created_at, modified_at, indexed_at, file_index, device_id
                FROM files WHERE content_hash = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let hash_str = hash.as_str();
        let result = stmt
            .query_row(params![hash_str], Self::map_row)
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(result)
    }

    /// Lists files with pagination
    pub fn list(conn: &Connection, limit: usize, offset: usize) -> Result<Vec<FileIdentity>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, path, mime_type, size_bytes, content_hash, status,
                       created_at, modified_at, indexed_at, file_index, device_id
                FROM files ORDER BY modified_at DESC LIMIT ?1 OFFSET ?2
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit as i64, offset as i64], |row| {
                Self::map_row(row)
            })
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut files = Vec::new();
        for file_res in rows {
            files.push(file_res.map_err(|e| NaviError::Database(e.to_string()))?);
        }
        Ok(files)
    }

    /// Deletes a file by its FileId
    pub fn delete(conn: &Connection, id: &FileId) -> Result<()> {
        let id_str = id.to_string();
        conn.execute("DELETE FROM files WHERE id = ?1", params![id_str])
            .map_err(|e| NaviError::Database(e.to_string()))?;
        Ok(())
    }

    /// Returns the total count of tracked files
    pub fn count(conn: &Connection) -> Result<usize> {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .map_err(|e| NaviError::Database(e.to_string()))?;
        Ok(count as usize)
    }

    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileIdentity> {
        let id_str: String = row.get(0)?;
        let path_str: String = row.get(1)?;
        let mime_str: String = row.get(2)?;
        let size: i64 = row.get(3)?;
        let hash_opt: Option<String> = row.get(4)?;
        let status_str: String = row.get(5)?;
        let created_str: String = row.get(6)?;
        let modified_str: String = row.get(7)?;
        let indexed_opt: Option<String> = row.get(8)?;
        let file_idx_opt: Option<i64> = row.get(9)?;
        let device_id_opt: Option<i64> = row.get(10)?;

        let id = FileId::parse(&id_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let fingerprint = PathFingerprint::from_path(std::path::Path::new(&path_str));
        let status: FileStatus =
            serde_json::from_str(&status_str).map_err(|_| rusqlite::Error::InvalidQuery)?;

        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&Utc);

        let modified_at = DateTime::parse_from_rfc3339(&modified_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&Utc);

        let indexed_at = indexed_opt.and_then(|i| {
            DateTime::parse_from_rfc3339(&i)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        });

        Ok(FileIdentity {
            id,
            fingerprint,
            mime_type: MimeType::new(mime_str),
            size_bytes: size as u64,
            content_hash: hash_opt.map(ContentHash::new),
            status,
            created_at,
            modified_at,
            indexed_at,
            file_index: file_idx_opt.map(|i| i as u64),
            device_id: device_id_opt.map(|d| d as u64),
        })
    }
}
