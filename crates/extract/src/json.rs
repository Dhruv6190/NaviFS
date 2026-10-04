//! JSON and JSONL document extractor with key/record chunking and index locators

use std::path::Path;
use async_trait::async_trait;
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput,
    FileChunk, FileIdentity, IndexLocator, LineRange, NaviError, RelationEdge, RelationType, Result,
};

/// JSON structural extractor handling both single JSON objects and line-delimited JSONL
pub struct JsonExtractor;

impl Default for JsonExtractor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentExtractor for JsonExtractor {
    fn name(&self) -> &str {
        "json-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.as_str() == "application/json"
            || identity.fingerprint.extension().map_or(false, |ext| ext == "json" || ext == "jsonl")
    }

    async fn extract(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        let raw = tokio::fs::read_to_string(path).await.map_err(|e| NaviError::ExtractionError {
            path: path.to_path_buf(),
            reason: format!("Failed reading JSON file: {}", e),
        })?;

        let mut chunks = Vec::new();
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let doc_entity = EntityNode::new(
            identity.fingerprint.filename(),
            EntityType::Document,
        ).with_file_id(identity.id);
        let doc_id = doc_entity.id;
        entities.push(doc_entity);

        // Check if JSON Lines (jsonl)
        let is_jsonl = identity.fingerprint.extension().map_or(false, |ext| ext == "jsonl");
        if is_jsonl {
            let mut byte_offset = 0u64;
            for (idx, line) in raw.lines().enumerate() {
                let trimmed = line.trim();
                let line_len = line.len() as u64 + 1;
                if trimmed.is_empty() {
                    byte_offset += line_len;
                    continue;
                }

                let line_num = idx + 1;
                let locator = IndexLocator::new(ByteRange::new(byte_offset, byte_offset + trimmed.len() as u64))
                    .with_lines(line_num, line_num);

                let chunk = FileChunk::new(
                    identity.id,
                    idx as u32,
                    ChunkType::JsonBlock,
                    locator.byte_range,
                    locator.line_range,
                    trimmed.to_string(),
                );
                chunks.push(chunk);
                byte_offset += line_len;
            }
            return Ok(ExtractionOutput { chunks, entities, relations });
        }

        // Standard JSON parsing
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(value) => {
                match value {
                    serde_json::Value::Object(map) => {
                        let mut chunk_index = 0u32;
                        let mut byte_offset = 0u64;

                        for (key, val) in map {
                            let formatted = serde_json::to_string_pretty(&val).unwrap_or_default();
                            let text = format!("\"{}\": {}", key, formatted);
                            let byte_len = text.len() as u64;

                            let chunk = FileChunk::new(
                                identity.id,
                                chunk_index,
                                ChunkType::JsonBlock,
                                ByteRange::new(byte_offset, byte_offset + byte_len),
                                None,
                                text,
                            );
                            chunks.push(chunk);

                            let key_entity = EntityNode::new(&key, EntityType::Topic)
                                .with_file_id(identity.id);
                            relations.push(RelationEdge::new(doc_id, key_entity.id, RelationType::Contains));
                            entities.push(key_entity);

                            chunk_index += 1;
                            byte_offset += byte_len;
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for (idx, item) in items.iter().enumerate() {
                            let text = serde_json::to_string_pretty(item).unwrap_or_default();
                            let byte_len = text.len() as u64;

                            let chunk = FileChunk::new(
                                identity.id,
                                idx as u32,
                                ChunkType::JsonBlock,
                                ByteRange::new(0, byte_len),
                                None,
                                text,
                            );
                            chunks.push(chunk);
                        }
                    }
                    _ => {
                        let chunk = FileChunk::new(
                            identity.id,
                            0,
                            ChunkType::JsonBlock,
                            ByteRange::new(0, raw.len() as u64),
                            Some(LineRange::new(1, raw.lines().count())),
                            raw,
                        );
                        chunks.push(chunk);
                    }
                }
            }
            Err(_e) => {
                // Fallback to text chunking if JSON is malformed
                let chunk = FileChunk::new(
                    identity.id,
                    0,
                    ChunkType::TextParagraph,
                    ByteRange::new(0, raw.len() as u64),
                    Some(LineRange::new(1, raw.lines().count())),
                    raw,
                );
                chunks.push(chunk);
            }
        }

        Ok(ExtractionOutput {
            chunks,
            entities,
            relations,
        })
    }
}
