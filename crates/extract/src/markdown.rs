//! Markdown structural extractor generating line/byte locators, heading entities, and hierarchy edges

use std::path::Path;
use async_trait::async_trait;
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput,
    FileChunk, FileIdentity, IndexLocator, NaviError, RelationEdge, RelationType, Result,
};

/// Markdown extractor breaking documents into structural sections with line bounds and graph nodes
pub struct MarkdownExtractor;

impl Default for MarkdownExtractor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentExtractor for MarkdownExtractor {
    fn name(&self) -> &str {
        "markdown-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.is_markdown()
    }

    async fn extract(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        let raw = tokio::fs::read_to_string(path).await.map_err(|e| NaviError::ExtractionError {
            path: path.to_path_buf(),
            reason: format!("Failed reading markdown file: {}", e),
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

        let lines: Vec<&str> = raw.lines().collect();
        let mut current_section_lines = Vec::new();
        let mut current_heading = "Preamble".to_string();
        let mut current_depth = 1u8;
        let mut section_start_line = 1;
        let mut chunk_index = 0u32;
        let mut current_byte_offset = 0u64;

        // Heading hierarchy stack: (depth, entity_id)
        let mut heading_stack: Vec<(u8, navifs_core::EntityId)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with('#') {
                // Flush previous section
                if !current_section_lines.is_empty() {
                    let section_content = current_section_lines.join("\n");
                    let byte_len = section_content.as_bytes().len() as u64;

                    let locator = IndexLocator::new(ByteRange::new(
                        current_byte_offset,
                        current_byte_offset + byte_len,
                    ))
                    .with_lines(section_start_line, line_num - 1);

                    let chunk = FileChunk::new(
                        identity.id,
                        chunk_index,
                        ChunkType::MarkdownSection {
                            heading: current_heading.clone(),
                            depth: current_depth,
                        },
                        locator.byte_range,
                        locator.line_range,
                        section_content,
                    );

                    chunks.push(chunk);
                    chunk_index += 1;
                    current_byte_offset += byte_len;
                    current_section_lines.clear();
                }

                let hash_count = trimmed.chars().take_while(|c| *c == '#').count();
                current_depth = hash_count as u8;
                current_heading = trimmed.trim_start_matches('#').trim().to_string();
                section_start_line = line_num;

                let heading_entity = EntityNode::new(&current_heading, EntityType::Heading)
                    .with_file_id(identity.id)
                    .with_properties(serde_json::json!({
                        "depth": current_depth,
                        "line": line_num,
                    }));
                let heading_id = heading_entity.id;
                entities.push(heading_entity);

                // Maintain heading hierarchy
                while let Some(&(parent_depth, _)) = heading_stack.last() {
                    if parent_depth >= current_depth {
                        heading_stack.pop();
                    } else {
                        break;
                    }
                }

                if let Some(&(_, parent_id)) = heading_stack.last() {
                    relations.push(RelationEdge::new(parent_id, heading_id, RelationType::Contains));
                } else {
                    relations.push(RelationEdge::new(doc_id, heading_id, RelationType::Contains));
                }

                heading_stack.push((current_depth, heading_id));
            } else {
                current_section_lines.push(*line);
            }
        }

        // Flush remaining section
        if !current_section_lines.is_empty() {
            let section_content = current_section_lines.join("\n");
            let byte_len = section_content.as_bytes().len() as u64;

            let locator = IndexLocator::new(ByteRange::new(
                current_byte_offset,
                current_byte_offset + byte_len,
            ))
            .with_lines(section_start_line, lines.len());

            let chunk = FileChunk::new(
                identity.id,
                chunk_index,
                ChunkType::MarkdownSection {
                    heading: current_heading,
                    depth: current_depth,
                },
                locator.byte_range,
                locator.line_range,
                section_content,
            );

            chunks.push(chunk);
        }

        Ok(ExtractionOutput {
            chunks,
            entities,
            relations,
        })
    }
}
