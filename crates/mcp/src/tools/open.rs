//! `open` tool: Bounded content range retrieval by lines, pages, byte offsets, or chunk index.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use navifs_core::{ByteRange, ChunkId, DatabaseStore, FileChunk, FileId, LineRange, PageRange};
use navifs_search::EvidenceBuilder;

#[derive(Debug, Deserialize)]
pub struct OpenArgs {
    pub file_id: Option<String>,
    pub path: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub start_page: Option<usize>,
    pub end_page: Option<usize>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub chunk_index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoundedContentResponse {
    pub file_id: FileId,
    pub path: String,
    pub filename: String,
    pub locator_summary: String,
    pub byte_range: ByteRange,
    pub line_range: Option<LineRange>,
    pub page_range: Option<PageRange>,
    pub content: String,
    pub total_lines: usize,
    pub chunks_included: Vec<ChunkId>,
}

pub struct OpenTool;

impl OpenTool {
    pub fn schema() -> Value {
        serde_json::json!({
            "name": "open",
            "description": "Retrieve bounded content ranges from an indexed file using line numbers, page numbers, byte offsets, or chunk indices, accompanied by exact index locators",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_id": {
                        "type": "string",
                        "description": "UUID of the target file"
                    },
                    "path": {
                        "type": "string",
                        "description": "Filesystem path of the target file"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "1-indexed starting line number of the bounded range"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "1-indexed ending line number of the bounded range"
                    },
                    "start_page": {
                        "type": "integer",
                        "description": "1-indexed starting page number (for PDF documents)"
                    },
                    "end_page": {
                        "type": "integer",
                        "description": "1-indexed ending page number (for PDF documents)"
                    },
                    "byte_start": {
                        "type": "integer",
                        "description": "Starting byte offset"
                    },
                    "byte_end": {
                        "type": "integer",
                        "description": "Ending byte offset"
                    },
                    "chunk_index": {
                        "type": "integer",
                        "description": "Specific chunk index within the file"
                    }
                }
            }
        })
    }

    pub async fn execute(
        db: &dyn DatabaseStore,
        args: OpenArgs,
    ) -> Result<BoundedContentResponse, String> {
        let file = if let Some(ref id_str) = args.file_id {
            let fid = FileId::parse(id_str).map_err(|e| format!("Invalid file_id: {}", e))?;
            db.get_file(&fid)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("File with id '{}' not found", id_str))?
        } else if let Some(ref path_str) = args.path {
            db.get_file_by_path(path_str)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("File with path '{}' not found", path_str))?
        } else {
            return Err("Either 'file_id' or 'path' must be provided to open".to_string());
        };

        let chunks = db
            .get_chunks_for_file(&file.id)
            .await
            .map_err(|e| e.to_string())?;

        let filename = file.fingerprint.filename().to_string();
        let path = file.fingerprint.as_str().to_string();

        if chunks.is_empty() {
            // Attempt reading from physical disk if file has not been chunked
            if let Ok(raw) = tokio::fs::read_to_string(&path).await {
                let lines: Vec<&str> = raw.lines().collect();
                let start_l = args.start_line.unwrap_or(1).max(1);
                let end_l = args.end_line.unwrap_or(lines.len()).min(lines.len()).max(start_l);

                let slice = if start_l <= lines.len() {
                    lines[start_l - 1..end_l].join("\n")
                } else {
                    String::new()
                };

                let line_range = Some(LineRange::new(start_l, end_l));
                let locator = EvidenceBuilder::format_locator_summary(&filename, line_range, None);

                return Ok(BoundedContentResponse {
                    file_id: file.id,
                    path,
                    filename,
                    locator_summary: locator,
                    byte_range: ByteRange::new(0, slice.len() as u64),
                    line_range,
                    page_range: None,
                    content: slice,
                    total_lines: end_l.saturating_sub(start_l) + 1,
                    chunks_included: Vec::new(),
                });
            }

            return Err(format!("File '{}' has no indexed content chunks", path));
        }

        // 1. Single chunk retrieval by chunk_index
        if let Some(c_idx) = args.chunk_index {
            if let Some(target) = chunks.iter().find(|c| c.chunk_index == c_idx) {
                let locator = EvidenceBuilder::format_locator_summary(
                    &filename,
                    target.line_range,
                    target.page_range,
                );
                return Ok(BoundedContentResponse {
                    file_id: file.id,
                    path,
                    filename,
                    locator_summary: locator,
                    byte_range: target.byte_range,
                    line_range: target.line_range,
                    page_range: target.page_range,
                    content: target.content.clone(),
                    total_lines: target.content.lines().count(),
                    chunks_included: vec![target.id],
                });
            } else {
                return Err(format!("Chunk index {} not found for file '{}'", c_idx, path));
            }
        }

        // 2. Filter chunks matching page bounds
        if args.start_page.is_some() || args.end_page.is_some() {
            let start_p = args.start_page.unwrap_or(1);
            let end_p = args.end_page.unwrap_or(start_p);

            let matching: Vec<&FileChunk> = chunks
                .iter()
                .filter(|c| {
                    if let Some(ref pr) = c.page_range {
                        pr.start_page <= end_p && pr.end_page >= start_p
                    } else {
                        false
                    }
                })
                .collect();

            if !matching.is_empty() {
                let combined_content = matching.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n\n");
                let min_byte = matching.first().map(|c| c.byte_range.start).unwrap_or(0);
                let max_byte = matching.last().map(|c| c.byte_range.end).unwrap_or(0);
                let min_line = matching.iter().filter_map(|c| c.line_range.map(|l| l.start_line)).min();
                let max_line = matching.iter().filter_map(|c| c.line_range.map(|l| l.end_line)).max();

                let line_range = match (min_line, max_line) {
                    (Some(s), Some(e)) => Some(LineRange::new(s, e)),
                    _ => None,
                };
                let page_range = Some(PageRange::new(start_p, end_p));
                let locator = EvidenceBuilder::format_locator_summary(&filename, line_range, page_range);

                return Ok(BoundedContentResponse {
                    file_id: file.id,
                    path,
                    filename,
                    locator_summary: locator,
                    byte_range: ByteRange::new(min_byte, max_byte),
                    line_range,
                    page_range,
                    content: combined_content,
                    total_lines: max_line.unwrap_or(0).saturating_sub(min_line.unwrap_or(0)) + 1,
                    chunks_included: matching.iter().map(|c| c.id).collect(),
                });
            }
        }

        // 3. Filter chunks matching line bounds
        if args.start_line.is_some() || args.end_line.is_some() {
            let start_l = args.start_line.unwrap_or(1);
            let end_l = args.end_line.unwrap_or(start_l + 100);

            let matching: Vec<&FileChunk> = chunks
                .iter()
                .filter(|c| {
                    if let Some(ref lr) = c.line_range {
                        lr.start_line <= end_l && lr.end_line >= start_l
                    } else {
                        false
                    }
                })
                .collect();

            if !matching.is_empty() {
                let combined_content = matching.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
                let min_byte = matching.first().map(|c| c.byte_range.start).unwrap_or(0);
                let max_byte = matching.last().map(|c| c.byte_range.end).unwrap_or(0);
                let line_range = Some(LineRange::new(start_l, end_l));
                let locator = EvidenceBuilder::format_locator_summary(&filename, line_range, None);

                return Ok(BoundedContentResponse {
                    file_id: file.id,
                    path,
                    filename,
                    locator_summary: locator,
                    byte_range: ByteRange::new(min_byte, max_byte),
                    line_range,
                    page_range: None,
                    content: combined_content,
                    total_lines: end_l.saturating_sub(start_l) + 1,
                    chunks_included: matching.iter().map(|c| c.id).collect(),
                });
            }
        }

        // 4. Filter chunks matching byte bounds
        if args.byte_start.is_some() || args.byte_end.is_some() {
            let start_b = args.byte_start.unwrap_or(0);
            let end_b = args.byte_end.unwrap_or(start_b + 4096);

            let matching: Vec<&FileChunk> = chunks
                .iter()
                .filter(|c| {
                    c.byte_range.start <= end_b && c.byte_range.end >= start_b
                })
                .collect();

            if !matching.is_empty() {
                let combined_content = matching.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
                let min_byte = matching.first().map(|c| c.byte_range.start).unwrap_or(start_b);
                let max_byte = matching.last().map(|c| c.byte_range.end).unwrap_or(end_b);
                let min_line = matching.iter().filter_map(|c| c.line_range.map(|l| l.start_line)).min();
                let max_line = matching.iter().filter_map(|c| c.line_range.map(|l| l.end_line)).max();
                let line_range = match (min_line, max_line) {
                    (Some(s), Some(e)) => Some(LineRange::new(s, e)),
                    _ => None,
                };
                let locator = EvidenceBuilder::format_locator_summary(&filename, line_range, None);

                return Ok(BoundedContentResponse {
                    file_id: file.id,
                    path,
                    filename,
                    locator_summary: locator,
                    byte_range: ByteRange::new(min_byte, max_byte),
                    line_range,
                    page_range: None,
                    content: combined_content,
                    total_lines: line_range.map(|l| l.end_line.saturating_sub(l.start_line) + 1).unwrap_or(0),
                    chunks_included: matching.iter().map(|c| c.id).collect(),
                });
            }
        }

        // 5. Default: Return first chunk
        let first = &chunks[0];
        let locator = EvidenceBuilder::format_locator_summary(&filename, first.line_range, first.page_range);

        Ok(BoundedContentResponse {
            file_id: file.id,
            path,
            filename,
            locator_summary: locator,
            byte_range: first.byte_range,
            line_range: first.line_range,
            page_range: first.page_range,
            content: first.content.clone(),
            total_lines: first.content.lines().count(),
            chunks_included: vec![first.id],
        })
    }
}
