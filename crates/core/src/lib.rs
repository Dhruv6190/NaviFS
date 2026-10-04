//! NaviFS Core Foundation Library
//!
//! Provides the primary domain primitives, identity models, cryptographic content hashes,
//! event specifications, and engine traits for NaviFS.

pub mod config;
pub mod error;
pub mod event;
pub mod foundation;
pub mod identity;
pub mod traits;

// Convenient flat re-exports
pub use config::{DatabaseConfig, EngineConfig, IndexingConfig, McpConfig, WatchDirectoryConfig};
pub use error::{NaviError, Result};
pub use event::{ChangeKind, FileTemporalEvent, FsEvent, IndexEvent};
pub use foundation::{ByteRange, ContentHash, IndexLocator, LineRange, MimeType, PageRange, PathFingerprint};
pub use identity::{
    ChunkId, ChunkType, EmbeddingId, EmbeddingRecord, EntityId, EntityNode, EntityType, FileChunk,
    FileId, FileIdentity, FileStatus, RelationEdge, RelationId, RelationType,
};
pub use traits::{DatabaseStore, DocumentExtractor, ExtractionOutput, SearchHit, SearchProvider};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_file_identity_creation() {
        let path = Path::new("crates/core/src/lib.rs");
        let mut file = FileIdentity::new(path, 1024, chrono::Utc::now());
        assert_eq!(file.fingerprint.extension(), Some("rs"));
        assert!(file.mime_type.is_code());
        assert_eq!(file.status, FileStatus::Discovered);

        let hash = ContentHash::from_bytes(b"fn main() {}");
        file = file.with_hash(hash.clone());
        assert_eq!(file.content_hash, Some(hash));

        file.mark_indexed();
        assert_eq!(file.status, FileStatus::Indexed);
        assert!(file.indexed_at.is_some());
    }

    #[test]
    fn test_chunk_creation() {
        let file_id = FileId::new();
        let chunk = FileChunk::new(
            file_id,
            0,
            ChunkType::CodeBlock {
                language: "rust".to_string(),
            },
            ByteRange::new(0, 100),
            Some(LineRange::new(1, 10)),
            "pub fn test() -> bool { true }".to_string(),
        );

        assert_eq!(chunk.file_id, file_id);
        assert_eq!(chunk.chunk_index, 0);
        assert!(!chunk.content.is_empty());
    }

    #[test]
    fn test_entity_and_relation_graph() {
        let file_id = FileId::new();
        let entity_a = EntityNode::new("UserStruct", EntityType::ClassOrStruct).with_file_id(file_id);
        let entity_b = EntityNode::new("UserRepository", EntityType::TraitOrInterface);

        let edge = RelationEdge::new(entity_a.id, entity_b.id, RelationType::Implements);
        assert_eq!(edge.source_id, entity_a.id);
        assert_eq!(edge.target_id, entity_b.id);
        assert_eq!(edge.relation_type, RelationType::Implements);
    }
}
