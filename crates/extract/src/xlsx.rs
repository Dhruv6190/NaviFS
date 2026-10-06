//! Spreadsheet document extractor for XLSX, XLS, and ODS workbooks
//!
//! Parses sheet names, columns, formulas/values, generates tabular Markdown chunks,
//! and maps sheet names to knowledge graph entities.

use async_trait::async_trait;
use calamine::{open_workbook_auto, Data, Reader};
use navifs_core::{
    ByteRange, ChunkType, DocumentExtractor, EntityNode, EntityType, ExtractionOutput, FileChunk,
    FileIdentity, IndexLocator, NaviError, RelationEdge, RelationType, Result,
};
use std::path::Path;
use tracing::{debug, warn};

/// Spreadsheet extractor parsing workbook sheets into tabular Markdown chunks
pub struct XlsxExtractor;

impl Default for XlsxExtractor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentExtractor for XlsxExtractor {
    fn name(&self) -> &str {
        "xlsx-extractor"
    }

    fn supports(&self, identity: &FileIdentity) -> bool {
        identity.mime_type.is_spreadsheet()
    }

    async fn extract(&self, identity: &FileIdentity, path: &Path) -> Result<ExtractionOutput> {
        let path_buf = path.to_path_buf();
        let file_id = identity.id;
        let filename = identity.fingerprint.filename().to_string();

        tokio::task::spawn_blocking(move || {
            let mut workbook =
                open_workbook_auto(&path_buf).map_err(|e| NaviError::ExtractionError {
                    path: path_buf.clone(),
                    reason: format!("Failed opening spreadsheet workbook: {}", e),
                })?;

            let mut chunks = Vec::new();
            let mut entities = Vec::new();
            let mut relations = Vec::new();

            // 1. Root Document Entity
            let doc_entity = EntityNode::new(&filename, EntityType::Document).with_file_id(file_id);
            let doc_id = doc_entity.id;
            entities.push(doc_entity);

            let sheet_names = workbook.sheet_names().to_vec();
            debug!(
                "Extracting {} sheets from {:?}",
                sheet_names.len(),
                path_buf
            );

            let mut chunk_index = 0u32;
            let mut current_byte_offset = 0u64;

            for sheet_name in sheet_names {
                // 2. Sheet Entity
                let sheet_entity = EntityNode::new(&sheet_name, EntityType::Heading)
                    .with_file_id(file_id)
                    .with_properties(serde_json::json!({
                        "sheet": sheet_name,
                        "type": "worksheet"
                    }));
                let sheet_id = sheet_entity.id;
                entities.push(sheet_entity);

                let relation =
                    RelationEdge::new(doc_id, sheet_id, RelationType::Contains).with_weight(1.0);
                relations.push(relation);

                // 3. Load Sheet Range
                let range = match workbook.worksheet_range(&sheet_name) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Error reading sheet '{}': {}", sheet_name, e);
                        continue;
                    }
                };

                let row_count = range.height();
                let col_count = range.width();

                if row_count == 0 || col_count == 0 {
                    continue;
                }

                // Extract column headers from first row
                let mut headers: Vec<String> = Vec::new();
                for col_idx in 0..col_count {
                    let cell_str = match range.get((0, col_idx)) {
                        Some(Data::String(s)) => s.trim().to_string(),
                        Some(Data::Int(i)) => i.to_string(),
                        Some(Data::Float(f)) => format!("{:.2}", f),
                        Some(Data::Bool(b)) => b.to_string(),
                        Some(Data::DateTime(d)) => format!("{:.2}", d),
                        _ => format!("Col{}", col_idx + 1),
                    };
                    headers.push(if cell_str.is_empty() {
                        format!("Col{}", col_idx + 1)
                    } else {
                        cell_str
                    });
                }

                // Chunk data rows in groups of 50 rows
                let chunk_size = 50;
                let data_rows = row_count.saturating_sub(1);
                let total_chunks = data_rows.div_ceil(chunk_size);

                for batch_idx in 0..total_chunks.max(1) {
                    let row_start = 1 + batch_idx * chunk_size;
                    let row_end = (row_start + chunk_size - 1)
                        .min(row_count.saturating_sub(1))
                        .max(row_start);

                    let mut table_md = String::new();
                    // Header line
                    table_md.push_str(&format!("### Sheet: {}\n\n", sheet_name));
                    table_md.push_str("| ");
                    table_md.push_str(&headers.join(" | "));
                    table_md.push_str(" |\n| ");
                    table_md.push_str(
                        &headers
                            .iter()
                            .map(|_| "---")
                            .collect::<Vec<_>>()
                            .join(" | "),
                    );
                    table_md.push_str(" |\n");

                    // Row data
                    for r in row_start..=row_end {
                        if r >= row_count {
                            break;
                        }
                        let mut row_vals = Vec::new();
                        for c in 0..col_count {
                            let val_str = match range.get((r, c)) {
                                Some(Data::String(s)) => s.replace('\n', " ").trim().to_string(),
                                Some(Data::Int(i)) => i.to_string(),
                                Some(Data::Float(f)) => format!("{:.2}", f),
                                Some(Data::Bool(b)) => b.to_string(),
                                Some(Data::DateTime(d)) => format!("{:.2}", d),
                                Some(Data::Empty) => "".to_string(),
                                Some(Data::Error(e)) => format!("ERR({:?})", e),
                                _ => "".to_string(),
                            };
                            row_vals.push(val_str);
                        }
                        table_md.push_str("| ");
                        table_md.push_str(&row_vals.join(" | "));
                        table_md.push_str(" |\n");
                    }

                    let byte_len = table_md.len() as u64;
                    let locator = IndexLocator::new(ByteRange::new(
                        current_byte_offset,
                        current_byte_offset + byte_len,
                    ))
                    .with_lines(row_start, row_end);

                    let chunk = FileChunk::new(
                        file_id,
                        chunk_index,
                        ChunkType::SpreadsheetSheet {
                            sheet: sheet_name.clone(),
                            row_start,
                            row_end,
                        },
                        locator.byte_range,
                        locator.line_range,
                        table_md,
                    );

                    chunks.push(chunk);
                    chunk_index += 1;
                    current_byte_offset += byte_len;
                }
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
