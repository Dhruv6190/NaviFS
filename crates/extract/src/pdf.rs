//! PDF document extractor with page-by-page text stream parsing, Flate decoding, and index locators

use async_trait::async_trait;
use flate2::read::ZlibDecoder;
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput, FileChunk,
    FileIdentity, IndexLocator, NaviError, PageRange, RelationEdge, RelationType, Result,
};
use std::io::Read;
use std::path::Path;
use tracing::debug;

/// PDF extractor that parses internal page objects, text streams, and computes page/line bounds
pub struct PdfExtractor;

impl Default for PdfExtractor {
    fn default() -> Self {
        Self
    }
}

impl PdfExtractor {
    /// Extracts text operators from a raw or decompressed PDF stream
    pub fn extract_text_from_stream(stream: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        let mut in_text_block = false;
        let mut current_line = String::new();

        let len = stream.len();
        let mut i = 0;

        while i < len {
            // Check for BT (Begin Text)
            if i + 2 <= len && &stream[i..i + 2] == b"BT" {
                let prev_ok = i == 0 || stream[i - 1].is_ascii_whitespace();
                let next_ok = i + 2 == len || stream[i + 2].is_ascii_whitespace();
                if prev_ok && next_ok {
                    in_text_block = true;
                    i += 2;
                    continue;
                }
            }

            // Check for ET (End Text)
            if i + 2 <= len && &stream[i..i + 2] == b"ET" {
                let prev_ok = i == 0 || stream[i - 1].is_ascii_whitespace();
                let next_ok = i + 2 == len || stream[i + 2].is_ascii_whitespace();
                if prev_ok && next_ok {
                    in_text_block = false;
                    if !current_line.trim().is_empty() {
                        lines.push(current_line.trim().to_string());
                        current_line.clear();
                    }
                    i += 2;
                    continue;
                }
            }

            if in_text_block {
                // Check for newline / positioning operators (T*, Td, TD)
                if i + 2 <= len
                    && (&stream[i..i + 2] == b"T*"
                        || &stream[i..i + 2] == b"Td"
                        || &stream[i..i + 2] == b"TD")
                {
                    let next_ok = i + 2 == len || stream[i + 2].is_ascii_whitespace();
                    if next_ok {
                        if !current_line.trim().is_empty() {
                            lines.push(current_line.trim().to_string());
                            current_line.clear();
                        }
                        i += 2;
                        continue;
                    }
                }

                // Check for literal string: (...) Tj or part of [...] TJ
                if stream[i] == b'(' {
                    i += 1;
                    let mut paren_depth = 1;
                    let mut literal = Vec::new();

                    while i < len && paren_depth > 0 {
                        if stream[i] == b'\\' && i + 1 < len {
                            match stream[i + 1] {
                                b'n' => literal.push(b'\n'),
                                b'r' => literal.push(b'\r'),
                                b't' => literal.push(b'\t'),
                                b'(' => literal.push(b'('),
                                b')' => literal.push(b')'),
                                b'\\' => literal.push(b'\\'),
                                _ => literal.push(stream[i + 1]),
                            }
                            i += 2;
                            continue;
                        } else if stream[i] == b'(' {
                            paren_depth += 1;
                        } else if stream[i] == b')' {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        literal.push(stream[i]);
                        i += 1;
                    }

                    if let Ok(text) = String::from_utf8(literal) {
                        current_line.push_str(&text);
                        current_line.push(' ');
                    }
                    continue;
                }

                // Check for hex string: <...> Tj
                if stream[i] == b'<' && i + 1 < len && stream[i + 1] != b'<' {
                    i += 1;
                    let mut hex_str = String::new();
                    while i < len && stream[i] != b'>' {
                        if !stream[i].is_ascii_whitespace() {
                            hex_str.push(stream[i] as char);
                        }
                        i += 1;
                    }
                    if i < len && stream[i] == b'>' {
                        i += 1;
                    }

                    if let Some(bytes) = Self::decode_hex(&hex_str) {
                        if let Ok(text) = String::from_utf8(bytes) {
                            current_line.push_str(&text);
                            current_line.push(' ');
                        }
                    }
                    continue;
                }
            }

            i += 1;
        }

        if !current_line.trim().is_empty() {
            lines.push(current_line.trim().to_string());
        }

        lines
    }

    /// Decodes a hexadecimal string into bytes
    pub fn decode_hex(hex: &str) -> Option<Vec<u8>> {
        let clean = hex.trim();
        if !clean.len().is_multiple_of(2) {
            return None;
        }
        (0..clean.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
            .collect()
    }

    /// Decompresses flate stream or returns raw bytes
    fn decompress_stream(data: &[u8]) -> Vec<u8> {
        let mut decoder = ZlibDecoder::new(data);
        let mut decompressed = Vec::new();
        if decoder.read_to_end(&mut decompressed).is_ok() && !decompressed.is_empty() {
            decompressed
        } else {
            data.to_vec()
        }
    }

    /// Parses page streams from PDF binary payload
    pub fn parse_pdf_pages(bytes: &[u8]) -> Vec<Vec<String>> {
        let mut pages = Vec::new();

        // Find all "stream ... endstream" segments
        let mut pos = 0;
        let len = bytes.len();

        while pos < len {
            let stream_kw = b"stream";
            let endstream_kw = b"endstream";

            if let Some(start_idx) = bytes[pos..]
                .windows(stream_kw.len())
                .position(|w| w == stream_kw)
            {
                let stream_start = pos + start_idx + stream_kw.len();
                // Skip CRLF after "stream"
                let mut data_start = stream_start;
                if data_start < len && bytes[data_start] == b'\r' {
                    data_start += 1;
                }
                if data_start < len && bytes[data_start] == b'\n' {
                    data_start += 1;
                }

                if let Some(end_idx) = bytes[data_start..]
                    .windows(endstream_kw.len())
                    .position(|w| w == endstream_kw)
                {
                    let data_end = data_start + end_idx;
                    let raw_stream = &bytes[data_start..data_end];

                    // Attempt decompression
                    let stream_bytes = Self::decompress_stream(raw_stream);
                    let lines = Self::extract_text_from_stream(&stream_bytes);

                    if !lines.is_empty() {
                        pages.push(lines);
                    }

                    pos = data_end + endstream_kw.len();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // If no streams contained extractable text, search for plain text chunks in ASCII
        if pages.is_empty() {
            let mut fallback_lines = Vec::new();
            let text_chunk = String::from_utf8_lossy(bytes);
            for line in text_chunk.lines() {
                let trimmed = line.trim();
                if trimmed.len() > 3
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                {
                    fallback_lines.push(trimmed.to_string());
                }
            }
            if !fallback_lines.is_empty() {
                pages.push(fallback_lines);
            }
        }

        pages
    }
}

#[async_trait]
impl DocumentExtractor for PdfExtractor {
    fn name(&self) -> &str {
        "pdf-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.as_str() == "application/pdf"
            || identity
                .fingerprint
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    }

    async fn extract(&self, identity: &FileIdentity, path: &Path) -> Result<ExtractionOutput> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| NaviError::ExtractionError {
                path: path.to_path_buf(),
                reason: format!("Failed reading PDF file bytes: {}", e),
            })?;

        let parsed_pages = Self::parse_pdf_pages(&bytes);
        let total_pages = parsed_pages.len().max(1);

        let mut chunks = Vec::new();
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        // 1. Create Document Entity Node
        let doc_entity = EntityNode::new(identity.fingerprint.filename(), EntityType::Document)
            .with_file_id(identity.id)
            .with_properties(serde_json::json!({
                "total_pages": total_pages,
                "format": "PDF",
                "size_bytes": bytes.len(),
            }));
        let doc_id = doc_entity.id;
        entities.push(doc_entity);

        let mut chunk_index = 0u32;
        let mut global_line_counter = 1usize;
        let mut byte_offset = 0u64;

        if parsed_pages.is_empty() {
            // Emits placeholder chunk if PDF text could not be extracted (e.g., scanned image)
            let notice = format!(
                "[PDF Document: {} (Total Pages: {}) - Image or binary content]",
                identity.fingerprint.filename(),
                total_pages
            );
            let byte_len = notice.len() as u64;

            let locator = IndexLocator::new(ByteRange::new(0, byte_len))
                .with_lines(1, 1)
                .with_pages(1, total_pages);

            let chunk = FileChunk::new(
                identity.id,
                0,
                ChunkType::PdfPageSection {
                    page: 1,
                    total_pages: Some(total_pages),
                },
                locator.byte_range,
                locator.line_range,
                notice,
            )
            .with_page_range(PageRange::new(1, total_pages));

            chunks.push(chunk);
            return Ok(ExtractionOutput {
                chunks,
                entities,
                relations,
            });
        }

        for (page_idx, page_lines) in parsed_pages.iter().enumerate() {
            let page_num = page_idx + 1;
            let page_line_count = page_lines.len().max(1);
            let page_start_line = global_line_counter;
            let page_end_line = global_line_counter + page_line_count - 1;
            global_line_counter = page_end_line + 1;

            let page_content = page_lines.join("\n");
            let page_byte_len = page_content.len() as u64;

            // 2. Create Page Entity Node and Relation
            let page_entity = EntityNode::new(format!("Page {}", page_num), EntityType::Section)
                .with_file_id(identity.id)
                .with_properties(serde_json::json!({
                    "page": page_num,
                    "lines": page_line_count,
                }));
            let page_id = page_entity.id;
            relations.push(RelationEdge::new(doc_id, page_id, RelationType::Contains));
            entities.push(page_entity);

            // 3. Build Chunk with exact Page and Line locators
            let locator =
                IndexLocator::new(ByteRange::new(byte_offset, byte_offset + page_byte_len))
                    .with_lines(page_start_line, page_end_line)
                    .with_pages(page_num, page_num);

            let chunk = FileChunk::new(
                identity.id,
                chunk_index,
                ChunkType::PdfPageSection {
                    page: page_num,
                    total_pages: Some(total_pages),
                },
                locator.byte_range,
                locator.line_range,
                page_content,
            )
            .with_page_range(PageRange::single(page_num));

            chunks.push(chunk);
            chunk_index += 1;
            byte_offset += page_byte_len;
        }

        debug!(
            "PdfExtractor: extracted {} chunks from {:?} ({} total pages)",
            chunks.len(),
            identity.fingerprint.filename(),
            total_pages
        );

        Ok(ExtractionOutput {
            chunks,
            entities,
            relations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_stream_literal_tj() {
        let stream = b"BT /F1 12 Tf 72 712 Td (Hello World) Tj ET";
        let lines = PdfExtractor::extract_text_from_stream(stream);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "Hello World");
    }

    #[test]
    fn test_extract_text_from_stream_with_escapes() {
        let stream = b"BT /F1 12 Tf (First Line\\nSecond Line) Tj T* (Third Line) Tj ET";
        let lines = PdfExtractor::extract_text_from_stream(stream);
        assert!(lines.iter().any(|l| l.contains("Third Line")));
    }
}
