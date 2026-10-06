use crate::error::Result;
use crate::event::FileTemporalEvent;
use crate::identity::{
    chunk::{ChunkId, FileChunk},
    embedding::EmbeddingRecord,
    entity::{EntityId, EntityNode},
    file::{FileId, FileIdentity},
    relation::RelationEdge,
};
use async_trait::async_trait;
use std::path::Path;

/// Produces dense vector embeddings for text, used for semantic retrieval
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Stable identifier of the model; stored with every vector so a model change is detectable
    fn model_id(&self) -> &str;

    /// Dimensionality of the produced vectors
    fn dimensions(&self) -> usize;

    /// Embeds document passages (indexing side). Output order matches input order.
    async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Embeds a search query (retrieval side)
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>>;
}

/// Trait implemented by content and metadata extractors
#[async_trait]
pub trait DocumentExtractor: Send + Sync {
    /// Identifier for this extractor (e.g., "markdown", "rust-ast", "plain-text")
    fn name(&self) -> &str;

    /// Checks if this extractor can handle the given file
    fn supports(&self, identity: &FileIdentity) -> bool;

    /// Extract chunks, entities, and relationships from the file
    async fn extract(&self, identity: &FileIdentity, path: &Path) -> Result<ExtractionOutput>;
}

/// Output package returned by an extraction pipeline
#[derive(Debug, Clone, Default)]
pub struct ExtractionOutput {
    pub chunks: Vec<FileChunk>,
    pub entities: Vec<EntityNode>,
    pub relations: Vec<RelationEdge>,
}

/// Storage interface for local-first metadata, chunks, and graph persistence
#[async_trait]
pub trait DatabaseStore: Send + Sync {
    async fn initialize(&self) -> Result<()>;

    // File operations
    async fn upsert_file(&self, file: &FileIdentity) -> Result<()>;
    async fn get_file(&self, id: &FileId) -> Result<Option<FileIdentity>>;
    async fn get_file_by_path(&self, path: &str) -> Result<Option<FileIdentity>>;
    async fn delete_file(&self, id: &FileId) -> Result<()>;
    async fn list_files(&self, limit: usize, offset: usize) -> Result<Vec<FileIdentity>>;

    // Chunk operations
    async fn save_chunks(&self, chunks: &[FileChunk]) -> Result<()>;
    async fn get_chunk(&self, id: &ChunkId) -> Result<Option<FileChunk>>;
    async fn get_chunks_for_file(&self, file_id: &FileId) -> Result<Vec<FileChunk>>;
    async fn delete_chunks_for_file(&self, file_id: &FileId) -> Result<()>;

    // Embedding operations
    async fn save_embeddings(&self, records: &[EmbeddingRecord]) -> Result<()>;
    /// True when the file already has vectors produced by the given model
    async fn has_embeddings_for_file(&self, file_id: &FileId, model_name: &str) -> Result<bool>;

    // Entity operations
    async fn save_entities(&self, entities: &[EntityNode]) -> Result<()>;
    async fn get_entity(&self, id: &EntityId) -> Result<Option<EntityNode>>;
    async fn get_entities_for_file(&self, file_id: &FileId) -> Result<Vec<EntityNode>>;

    // Relation operations
    async fn save_relations(&self, relations: &[RelationEdge]) -> Result<()>;
    async fn get_relations_for_entity(&self, entity_id: &EntityId) -> Result<Vec<RelationEdge>>;

    // Temporal Event operations (§13 & §17.1)
    async fn record_event(&self, event: &FileTemporalEvent) -> Result<()>;
    async fn get_events_for_file(
        &self,
        file_id: &FileId,
        limit: usize,
    ) -> Result<Vec<FileTemporalEvent>>;
}

/// Search engine interface providing lexical, vector, and hybrid search
#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn index_chunks(&self, chunks: &[FileChunk]) -> Result<()>;
    async fn remove_file_from_index(&self, file_id: &FileId) -> Result<()>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>>;
}

/// Individual hit returned from a search query
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub chunk_id: uuid::Uuid,
    pub file_id: uuid::Uuid,
    pub score: f32,
    pub content: String,
    pub path: String,
    pub chunk_index: u32,
}
