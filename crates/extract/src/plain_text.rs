//! Plain-text document extractor with sliding window chunker and index locators

use async_trait::async_trait;
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, ExtractionOutput, FileChunk, FileIdentity,
    IndexLocator, NaviError, Result,
};
use std::path::Path;

/// Universal plain-text extractor with sliding window chunker and exact line/byte locators
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
        identity.mime_type.is_text() || identity.mime_type.is_code()
    }

    async fn extract(&self, identity: &FileIdentity, path: &Path) -> Result<ExtractionOutput> {
        let content =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| NaviError::ExtractionError {
                    path: path.to_path_buf(),
                    reason: format!("Failed to read plain text file: {}", e),
                })?;

        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return Ok(ExtractionOutput::default());
        }

        let mut current_lines = Vec::new();
        let mut current_chars = 0;
        let mut start_line_num = 1;
        let mut chunk_index = 0u32;
        let mut byte_offset = 0u64;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            current_lines.push(*line);
            current_chars += line.len() + 1; // +1 for newline

            if current_chars >= self.chunk_size || line_num == lines.len() {
                let chunk_text = current_lines.join("\n");
                let byte_len = chunk_text.len() as u64;

                let locator =
                    IndexLocator::new(ByteRange::new(byte_offset, byte_offset + byte_len))
                        .with_lines(start_line_num, line_num);

                let chunk = FileChunk::new(
                    identity.id,
                    chunk_index,
                    ChunkType::TextParagraph,
                    locator.byte_range,
                    locator.line_range,
                    chunk_text,
                );

                chunks.push(chunk);
                chunk_index += 1;
                byte_offset += byte_len;

                // Prepare next chunk with overlap
                let overlap_lines_count = (self.chunk_overlap / 60).max(1).min(current_lines.len());
                let next_lines =
                    current_lines[current_lines.len() - overlap_lines_count..].to_vec();
                start_line_num = line_num.saturating_sub(overlap_lines_count) + 1;
                current_chars = next_lines.iter().map(|l| l.len() + 1).sum();
                current_lines = next_lines;
            }
        }

        Ok(ExtractionOutput {
            chunks,
            entities: Vec::new(),
            relations: Vec::new(),
        })
    }
}
