//! DOCX document extractor with OpenXML structure parsing, headings, tables, and graph nodes
//!
//! Unpacks word/document.xml, identifies heading levels and table structures,
//! produces formatted Markdown section chunks, and populates knowledge graph entities.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use tracing::debug;
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput,
    FileChunk, FileIdentity, IndexLocator, NaviError, RelationEdge, RelationType, Result,
};

/// DOCX document extractor that parses XML bodies, headings, paragraphs, and tables
pub struct DocxExtractor;

impl Default for DocxExtractor {
    fn default() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
enum DocxElement {
    Heading { text: String, depth: u8 },
    Paragraph(String),
    Table(Vec<Vec<String>>),
}

impl DocxExtractor {
    /// Parses word/document.xml from a DOCX zip archive into structured elements
    fn parse_docx_xml(xml_content: &str) -> Vec<DocxElement> {
        let mut reader = Reader::from_str(xml_content);
        reader.config_mut().trim_text(true);

        let mut elements = Vec::new();
        let mut in_p = false;
        let mut in_tc = false;
        let mut in_t = false;
        let mut current_p_text = String::new();
        let mut current_heading_depth: Option<u8> = None;

        let mut current_table: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_cell_text = String::new();

        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let local = e.local_name();
                    match local.as_ref() {
                        b"tbl" => {
                            current_table.clear();
                        }
                        b"tr" => {
                            current_row.clear();
                        }
                        b"tc" => {
                            in_tc = true;
                            current_cell_text.clear();
                        }
                        b"p" => {
                            in_p = true;
                            current_p_text.clear();
                            current_heading_depth = None;
                        }
                        b"pStyle" => {
                            // Inspect w:val attribute for heading styles
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"val" {
                                    let val = String::from_utf8_lossy(&attr.value).to_lowercase();
                                    if val.contains("heading") {
                                        if let Some(digit) = val.chars().find(|c| c.is_ascii_digit()) {
                                            if let Some(d) = digit.to_digit(10) {
                                                current_heading_depth = Some(d as u8);
                                            }
                                        } else {
                                            current_heading_depth = Some(1);
                                        }
                                    } else if val == "title" {
                                        current_heading_depth = Some(1);
                                    } else if val == "subtitle" {
                                        current_heading_depth = Some(2);
                                    }
                                }
                            }
                        }
                        b"t" => {
                            in_t = true;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let local = e.local_name();
                    if local.as_ref() == b"pStyle" {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                let val = String::from_utf8_lossy(&attr.value).to_lowercase();
                                if val.contains("heading") {
                                    if let Some(digit) = val.chars().find(|c| c.is_ascii_digit()) {
                                        if let Some(d) = digit.to_digit(10) {
                                            current_heading_depth = Some(d as u8);
                                        }
                                    } else {
                                        current_heading_depth = Some(1);
                                    }
                                } else if val == "title" {
                                    current_heading_depth = Some(1);
                                } else if val == "subtitle" {
                                    current_heading_depth = Some(2);
                                }
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_t {
                        if let Ok(text) = e.unescape() {
                            if in_tc {
                                current_cell_text.push_str(&text);
                                current_cell_text.push(' ');
                            } else if in_p {
                                current_p_text.push_str(&text);
                                current_p_text.push(' ');
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let local = e.local_name();
                    match local.as_ref() {
                        b"t" => in_t = false,
                        b"p" => {
                            in_p = false;
                            let trimmed = current_p_text.trim().to_string();
                            if !trimmed.is_empty() && !in_tc {
                                if let Some(depth) = current_heading_depth {
                                    elements.push(DocxElement::Heading { text: trimmed, depth });
                                } else {
                                    elements.push(DocxElement::Paragraph(trimmed));
                                }
                            }
                            current_p_text.clear();
                            current_heading_depth = None;
                        }
                        b"tc" => {
                            in_tc = false;
                            current_row.push(current_cell_text.trim().to_string());
                            current_cell_text.clear();
                        }
                        b"tr" => {
                            if !current_row.is_empty() {
                                current_table.push(current_row.clone());
                            }
                            current_row.clear();
                        }
                        b"tbl" => {
                            if !current_table.is_empty() {
                                elements.push(DocxElement::Table(current_table.clone()));
                            }
                            current_table.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    debug!("XML parse event error in docx: {}", e);
                    break;
                }
                _ => {}
            }
            buf.clear();
        }

        elements
    }
}

#[async_trait]
impl DocumentExtractor for DocxExtractor {
    fn name(&self) -> &str {
        "docx-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.is_docx()
    }

    async fn extract(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        let path_buf = path.to_path_buf();
        let file_id = identity.id;
        let filename = identity.fingerprint.filename().to_string();

        tokio::task::spawn_blocking(move || {
            let file = File::open(&path_buf).map_err(|e| NaviError::ExtractionError {
                path: path_buf.clone(),
                reason: format!("Failed opening DOCX file: {}", e),
            })?;

            let mut archive = zip::ZipArchive::new(file).map_err(|e| NaviError::ExtractionError {
                path: path_buf.clone(),
                reason: format!("Failed reading DOCX zip archive: {}", e),
            })?;

            let mut doc_xml = String::new();
            {
                let mut xml_file = archive.by_name("word/document.xml").map_err(|e| NaviError::ExtractionError {
                    path: path_buf.clone(),
                    reason: format!("word/document.xml not found in DOCX: {}", e),
                })?;
                xml_file.read_to_string(&mut doc_xml).map_err(|e| NaviError::ExtractionError {
                    path: path_buf.clone(),
                    reason: format!("Failed reading word/document.xml: {}", e),
                })?;
            }

            let elements = Self::parse_docx_xml(&doc_xml);

            let mut chunks = Vec::new();
            let mut entities = Vec::new();
            let mut relations = Vec::new();

            // 1. Root Document Node
            let doc_entity = EntityNode::new(&filename, EntityType::Document).with_file_id(file_id);
            let doc_id = doc_entity.id;
            entities.push(doc_entity);

            let mut current_heading = "Preamble".to_string();
            let mut current_depth = 1u8;
            let mut current_section_lines = Vec::new();
            let mut chunk_index = 0u32;
            let mut current_byte_offset = 0u64;
            let mut current_line_num = 1usize;
            let mut section_start_line = 1usize;

            // Heading stack for hierarchy tracking: (depth, entity_id)
            let mut heading_stack: Vec<(u8, navifs_core::EntityId)> = Vec::new();

            for elem in elements {
                match elem {
                    DocxElement::Heading { text, depth } => {
                        // Flush previous section
                        if !current_section_lines.is_empty() {
                            let content = current_section_lines.join("\n\n");
                            let byte_len = content.as_bytes().len() as u64;

                            let locator = IndexLocator::new(ByteRange::new(
                                current_byte_offset,
                                current_byte_offset + byte_len,
                            ))
                            .with_lines(section_start_line, current_line_num.saturating_sub(1));

                            chunks.push(FileChunk::new(
                                file_id,
                                chunk_index,
                                ChunkType::DocxSection {
                                    heading: current_heading.clone(),
                                    depth: current_depth,
                                },
                                locator.byte_range,
                                locator.line_range,
                                content,
                            ));

                            chunk_index += 1;
                            current_byte_offset += byte_len;
                            current_section_lines.clear();
                        }

                        current_heading = text.clone();
                        current_depth = depth;
                        section_start_line = current_line_num;

                        let heading_entity = EntityNode::new(&text, EntityType::Heading)
                            .with_file_id(file_id)
                            .with_properties(serde_json::json!({
                                "depth": depth,
                                "type": "docx_heading"
                            }));
                        let heading_id = heading_entity.id;
                        entities.push(heading_entity);

                        // Link hierarchy
                        while let Some((parent_depth, _)) = heading_stack.last() {
                            if *parent_depth >= depth {
                                heading_stack.pop();
                            } else {
                                break;
                            }
                        }

                        let parent_id = heading_stack
                            .last()
                            .map(|(_, id)| *id)
                            .unwrap_or(doc_id);

                        relations.push(
                            RelationEdge::new(parent_id, heading_id, RelationType::Contains)
                                .with_weight(1.0),
                        );
                        heading_stack.push((depth, heading_id));

                        current_line_num += 1;
                    }
                    DocxElement::Paragraph(p) => {
                        current_section_lines.push(p);
                        current_line_num += 1;
                    }
                    DocxElement::Table(tbl) => {
                        if !tbl.is_empty() {
                            let mut md_tbl = String::new();
                            let first_row = &tbl[0];
                            md_tbl.push_str("| ");
                            md_tbl.push_str(&first_row.join(" | "));
                            md_tbl.push_str(" |\n| ");
                            md_tbl.push_str(&first_row.iter().map(|_| "---").collect::<Vec<_>>().join(" | "));
                            md_tbl.push_str(" |\n");

                            for row in &tbl[1..] {
                                md_tbl.push_str("| ");
                                md_tbl.push_str(&row.join(" | "));
                                md_tbl.push_str(" |\n");
                            }

                            current_section_lines.push(md_tbl);
                            current_line_num += tbl.len() + 1;
                        }
                    }
                }
            }

            // Flush final section
            if !current_section_lines.is_empty() {
                let content = current_section_lines.join("\n\n");
                let byte_len = content.as_bytes().len() as u64;

                let locator = IndexLocator::new(ByteRange::new(
                    current_byte_offset,
                    current_byte_offset + byte_len,
                ))
                .with_lines(section_start_line, current_line_num);

                chunks.push(FileChunk::new(
                    file_id,
                    chunk_index,
                    ChunkType::DocxSection {
                        heading: current_heading,
                        depth: current_depth,
                    },
                    locator.byte_range,
                    locator.line_range,
                    content,
                ));
            }

            Ok(ExtractionOutput {
                chunks,
                entities,
                relations,
            })
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }
}
