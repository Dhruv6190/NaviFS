//! Extraction subsystem for NaviFS
//!
//! Provides content parsing, semantic windowed chunking, and structural entity
//! extraction for markdown, source code, and structured plain-text files.

use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;
use tracing::{debug, info};
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput,
    FileChunk, FileIdentity, LineRange, NaviError, RelationEdge, RelationType, Result,
};

/// Universal plain-text extractor with sliding window chunker
pub struct PlainTextExtractor {
    chunk_size: usize,
    chunk_overlap: usize,
}

impl PlainTextExtractor {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            chunk_size: chunk_size.max(100),
            chunk_overlap: chunk_overlap.min(chunk_size / 2),
        }
    }
}

impl Default for PlainTextExtractor {
    fn default() -> Self {
        Self::new(1200, 150)
    }
}

#[async_trait]
impl DocumentExtractor for PlainTextExtractor {
    fn name(&self) -> &str {
        "plain-text-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.is_text()
    }

    async fn extract(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| NaviError::ExtractionError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let total_chars = chars.len();

        let mut start_idx = 0;
        let mut chunk_idx = 0;

        while start_idx < total_chars {
            let end_idx = (start_idx + self.chunk_size).min(total_chars);
            let chunk_str: String = chars[start_idx..end_idx].iter().collect();

            // Calculate rough byte ranges
            let byte_start = chars[..start_idx].iter().map(|c| c.len_utf8() as u64).sum();
            let byte_len: u64 = chunk_str.as_bytes().len() as u64;

            let chunk = FileChunk::new(
                identity.id,
                chunk_idx,
                ChunkType::TextParagraph,
                ByteRange::new(byte_start, byte_start + byte_len),
                None,
                chunk_str,
            );
            chunks.push(chunk);

            if end_idx == total_chars {
                break;
            }
            start_idx += self.chunk_size - self.chunk_overlap;
            chunk_idx += 1;
        }

        Ok(ExtractionOutput {
            chunks,
            entities: Vec::new(),
            relations: Vec::new(),
        })
    }
}

/// Markdown structural extractor extracting headings and document sections
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
            reason: e.to_string(),
        })?;

        let mut chunks = Vec::new();
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let lines: Vec<&str> = raw.lines().collect();
        let mut current_section = Vec::new();
        let mut current_heading = "Introduction".to_string();
        let mut current_depth = 1u8;
        let mut section_start_line = 1;
        let mut chunk_index = 0u32;

        let doc_entity = EntityNode::new(
            identity.fingerprint.filename(),
            EntityType::Document,
        ).with_file_id(identity.id);
        let doc_id = doc_entity.id;
        entities.push(doc_entity);

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with('#') {
                if !current_section.is_empty() {
                    let section_content = current_section.join("\n");
                    let chunk = FileChunk::new(
                        identity.id,
                        chunk_index,
                        ChunkType::MarkdownSection {
                            heading: current_heading.clone(),
                            depth: current_depth,
                        },
                        ByteRange::new(0, section_content.len() as u64),
                        Some(LineRange::new(section_start_line, line_num - 1)),
                        section_content,
                    );
                    chunks.push(chunk);
                    chunk_index += 1;
                    current_section.clear();
                }

                let hashes = trimmed.chars().take_while(|c| *c == '#').count();
                current_depth = hashes as u8;
                current_heading = trimmed.trim_start_matches('#').trim().to_string();
                section_start_line = line_num;

                let heading_entity = EntityNode::new(&current_heading, EntityType::Heading)
                    .with_file_id(identity.id)
                    .with_properties(serde_json::json!({ "depth": current_depth, "line": line_num }));
                
                let rel = RelationEdge::new(doc_id, heading_entity.id, RelationType::Contains);
                entities.push(heading_entity);
                relations.push(rel);
            } else {
                current_section.push(*line);
            }
        }

        if !current_section.is_empty() {
            let section_content = current_section.join("\n");
            let chunk = FileChunk::new(
                identity.id,
                chunk_index,
                ChunkType::MarkdownSection {
                    heading: current_heading,
                    depth: current_depth,
                },
                ByteRange::new(0, section_content.len() as u64),
                Some(LineRange::new(section_start_line, lines.len())),
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

/// Source code extractor extracting top-level symbols (functions, structs, traits)
pub struct CodeExtractor;

impl Default for CodeExtractor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentExtractor for CodeExtractor {
    fn name(&self) -> &str {
        "code-symbol-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.is_code()
    }

    async fn extract(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| NaviError::ExtractionError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

        let mut chunks = Vec::new();
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let ext = identity.fingerprint.extension().unwrap_or("txt").to_string();

        let file_entity = EntityNode::new(
            identity.fingerprint.filename(),
            EntityType::File,
        ).with_file_id(identity.id);
        let file_node_id = file_entity.id;
        entities.push(file_entity);

        // Extract functions and structs via pattern analysis
        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") || trimmed.starts_with("def ") {
                let name = trimmed
                    .split('(')
                    .next()
                    .and_then(|s| s.split_whitespace().last())
                    .unwrap_or("unknown_fn");

                let func_entity = EntityNode::new(name, EntityType::Function)
                    .with_file_id(identity.id)
                    .with_properties(serde_json::json!({ "line": line_num }));

                let rel = RelationEdge::new(file_node_id, func_entity.id, RelationType::Defines);
                entities.push(func_entity);
                relations.push(rel);
            } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") || trimmed.starts_with("class ") {
                let name = trimmed
                    .split_whitespace()
                    .nth(if trimmed.starts_with("pub") { 2 } else { 1 })
                    .map(|s| s.trim_end_matches('{'))
                    .unwrap_or("unknown_struct");

                let struct_entity = EntityNode::new(name, EntityType::ClassOrStruct)
                    .with_file_id(identity.id)
                    .with_properties(serde_json::json!({ "line": line_num }));

                let rel = RelationEdge::new(file_node_id, struct_entity.id, RelationType::Defines);
                entities.push(struct_entity);
                relations.push(rel);
            }
        }

        // Standard chunking for code
        let chunk = FileChunk::new(
            identity.id,
            0,
            ChunkType::CodeBlock { language: ext },
            ByteRange::new(0, content.len() as u64),
            Some(LineRange::new(1, content.lines().count())),
            content,
        );
        chunks.push(chunk);

        Ok(ExtractionOutput {
            chunks,
            entities,
            relations,
        })
    }
}

/// Extractor Registry that dispatches extraction to the highest-priority supported engine
pub struct ExtractorRegistry {
    extractors: Vec<Arc<dyn DocumentExtractor>>,
    fallback: Arc<PlainTextExtractor>,
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        let fallback = Arc::new(PlainTextExtractor::default());
        let extractors: Vec<Arc<dyn DocumentExtractor>> = vec![
            Arc::new(MarkdownExtractor),
            Arc::new(CodeExtractor),
            fallback.clone(),
        ];

        Self {
            extractors,
            fallback,
        }
    }

    pub fn register(&mut self, extractor: Arc<dyn DocumentExtractor>) {
        self.extractors.insert(0, extractor);
    }

    pub async fn extract_file(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        for ext in &self.extractors {
            if ext.supports(identity) {
                debug!("Selected extractor [{}] for {}", ext.name(), identity.fingerprint.as_str());
                return ext.extract(identity, path).await;
            }
        }

        self.fallback.extract(identity, path).await
    }
}
