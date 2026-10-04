//! Local-first SQLite database layer for NaviFS
//!
//! Exposes migration execution, entity repositories, and SQLite FTS5 full-text lexical search.

pub mod migrations;
pub mod repository;

pub use migrations::run_migrations;
pub use repository::{
    ChunkRepository, EmbeddingRepository, FileRepository, FtsRepository, FtsSearchResult,
    RelationRepository, VectorMatch,
};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rusqlite::Connection;
use tracing::info;
use navifs_core::{
    ChunkId, DatabaseStore, EmbeddingRecord, EntityId, EntityNode, FileChunk, FileId, FileIdentity,
    NaviError, RelationEdge, Result,
};

/// High-performance thread-safe SQLite database manager for NaviFS
#[derive(Clone)]
pub struct SqliteDatabase {
    db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl SqliteDatabase {
    /// Opens or creates SQLite database file and enables WAL mode
    pub fn new(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(NaviError::Io)?;
        }
        let conn = Connection::open(&db_path).map_err(|e| NaviError::Database(e.to_string()))?;

        // Enable Write-Ahead-Logging (WAL) for high concurrent throughput
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(Self {
            db_path,
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Creates an in-memory SQLite database instance (primarily for tests)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| NaviError::Database(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(Self {
            db_path: PathBuf::from(":memory:"),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Returns the database file path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Executes embedded migrations creating tables for files, contentions (content_chunks), embeddings, and relationships
    pub async fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            run_migrations(&mut conn)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    // --- Vector Embeddings Repository Methods ---

    pub async fn save_embeddings(&self, records: &[EmbeddingRecord]) -> Result<()> {
        let conn = self.conn.clone();
        let records = records.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            EmbeddingRepository::save_batch(&mut conn, &records)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    pub async fn find_nearest_neighbors(
        &self,
        query_vector: &[f32],
        model_name: &str,
        limit: usize,
    ) -> Result<Vec<VectorMatch>> {
        let conn = self.conn.clone();
        let query_vector = query_vector.to_vec();
        let model_name = model_name.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            EmbeddingRepository::find_nearest_neighbors(&conn, &query_vector, &model_name, limit)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    // --- SQLite FTS5 Full-Text Lexical Search Methods ---

    pub async fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let conn = self.conn.clone();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FtsRepository::search(&conn, &query, limit)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    pub async fn fts_search_paths(&self, term: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let conn = self.conn.clone();
        let term = term.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FtsRepository::search_by_filename_or_path(&conn, &term, limit)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }
}

#[async_trait]
impl DatabaseStore for SqliteDatabase {
    async fn initialize(&self) -> Result<()> {
        info!("Running database schema migrations...");
        self.run_migrations().await?;
        info!("Database successfully initialized with FTS5 lexical index");
        Ok(())
    }

    // File operations
    async fn upsert_file(&self, file: &FileIdentity) -> Result<()> {
        let conn = self.conn.clone();
        let file = file.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FileRepository::upsert(&conn, &file)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_file(&self, id: &FileId) -> Result<Option<FileIdentity>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FileRepository::get_by_id(&conn, &id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_file_by_path(&self, path: &str) -> Result<Option<FileIdentity>> {
        let conn = self.conn.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FileRepository::get_by_path(&conn, &path)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn delete_file(&self, id: &FileId) -> Result<()> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FileRepository::delete(&conn, &id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn list_files(&self, limit: usize, offset: usize) -> Result<Vec<FileIdentity>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            FileRepository::list(&conn, limit, offset)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    // Chunk operations (content_chunks / contentions)
    async fn save_chunks(&self, chunks: &[FileChunk]) -> Result<()> {
        let conn = self.conn.clone();
        let chunks = chunks.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            ChunkRepository::save_batch(&mut conn, &chunks)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_chunk(&self, id: &ChunkId) -> Result<Option<FileChunk>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            ChunkRepository::get_by_id(&conn, &id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_chunks_for_file(&self, file_id: &FileId) -> Result<Vec<FileChunk>> {
        let conn = self.conn.clone();
        let file_id = *file_id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            ChunkRepository::get_for_file(&conn, &file_id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn delete_chunks_for_file(&self, file_id: &FileId) -> Result<()> {
        let conn = self.conn.clone();
        let file_id = *file_id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            ChunkRepository::delete_for_file(&conn, &file_id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    // Entity operations
    async fn save_entities(&self, entities: &[EntityNode]) -> Result<()> {
        let conn = self.conn.clone();
        let entities = entities.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            RelationRepository::save_entities(&mut conn, &entities)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_entity(&self, id: &EntityId) -> Result<Option<EntityNode>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            RelationRepository::get_entity(&conn, &id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_entities_for_file(&self, file_id: &FileId) -> Result<Vec<EntityNode>> {
        let conn = self.conn.clone();
        let file_id = *file_id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            RelationRepository::get_entities_for_file(&conn, &file_id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    // Relation operations
    async fn save_relations(&self, relations: &[RelationEdge]) -> Result<()> {
        let conn = self.conn.clone();
        let relations = relations.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            RelationRepository::save_relationships(&mut conn, &relations)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    async fn get_relations_for_entity(&self, entity_id: &EntityId) -> Result<Vec<RelationEdge>> {
        let conn = self.conn.clone();
        let entity_id = *entity_id;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| NaviError::Database(e.to_string()))?;
            RelationRepository::get_relationships(&conn, &entity_id)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use navifs_core::{ByteRange, ChunkType, ContentHash, LineRange};

    #[tokio::test]
    async fn test_full_database_lifecycle_and_fts() {
        let db = SqliteDatabase::in_memory().expect("In-memory database creation failed");
        db.initialize().await.expect("Migrations failed");

        // 1. Insert file
        let path = Path::new("crates/core/src/identity/file.rs");
        let mut file = FileIdentity::new(path, 2048, chrono::Utc::now());
        let hash = ContentHash::from_bytes(b"content for test");
        file = file.with_hash(hash.clone());
        db.upsert_file(&file).await.expect("Failed to upsert file");

        // 2. Insert chunk
        let chunk = FileChunk::new(
            file.id,
            0,
            ChunkType::CodeBlock { language: "rust".to_string() },
            ByteRange::new(0, 100),
            Some(LineRange::new(1, 10)),
            "pub struct FileIdentity { pub id: FileId }".to_string(),
        );
        db.save_chunks(&[chunk.clone()]).await.expect("Failed to save chunk");

        // 3. Insert embedding
        let embedding = EmbeddingRecord::new(
            file.id,
            Some(chunk.id),
            "text-embedding-3-small",
            vec![0.1, 0.2, 0.3, 0.4],
        );
        db.save_embeddings(&[embedding]).await.expect("Failed to save embeddings");

        // 4. Test FTS5 full-text search across content and filename
        let search_hits = db.fts_search("FileIdentity", 10).await.expect("FTS search failed");
        assert!(!search_hits.is_empty(), "Expected at least 1 FTS hit for FileIdentity");
        assert_eq!(search_hits[0].file_id, file.id);

        // 5. Test vector similarity
        let query_vec = vec![0.1, 0.2, 0.3, 0.4];
        let matches = db.find_nearest_neighbors(&query_vec, "text-embedding-3-small", 5).await.expect("Vector search failed");
        assert_eq!(matches.len(), 1);
        assert!((matches[0].similarity - 1.0).abs() < 1e-4);
    }
}
