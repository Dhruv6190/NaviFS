//! Evidence resolution and locator formatting for LLMs and user inspection

use crate::fusion::FusedCandidate;
use crate::types::EvidenceLookup;
use navifs_core::{ByteRange, DatabaseStore, FileChunk, FileIdentity, LineRange, PageRange};

/// Formats human and machine-readable evidence summaries with exact line and page locators
pub struct EvidenceBuilder;

impl EvidenceBuilder {
    /// Formats a standardized locator summary string
    pub fn format_locator_summary(
        filename: &str,
        line_range: Option<LineRange>,
        page_range: Option<PageRange>,
    ) -> String {
        match (page_range, line_range) {
            (Some(page), Some(line)) => {
                if page.start_page == page.end_page {
                    format!(
                        "{}:Page {} (Lines {}-{})",
                        filename, page.start_page, line.start_line, line.end_line
                    )
                } else {
                    format!(
                        "{}:Pages {}-{} (Lines {}-{})",
                        filename, page.start_page, page.end_page, line.start_line, line.end_line
                    )
                }
            }
            (Some(page), None) => {
                if page.start_page == page.end_page {
                    format!("{}:Page {}", filename, page.start_page)
                } else {
                    format!("{}:Pages {}-{}", filename, page.start_page, page.end_page)
                }
            }
            (None, Some(line)) => {
                if line.start_line == line.end_line {
                    format!("{}:Line {}", filename, line.start_line)
                } else {
                    format!("{}:Lines {}-{}", filename, line.start_line, line.end_line)
                }
            }
            (None, None) => format!("{}:Document", filename),
        }
    }

    /// Resolves full chunk and file evidence from the database for a fused candidate
    pub async fn resolve_evidence(
        db: &dyn DatabaseStore,
        candidate: &FusedCandidate,
        file: Option<&FileIdentity>,
        chunk: Option<&FileChunk>,
    ) -> EvidenceLookup {
        let filename = file
            .map(|f| f.fingerprint.filename().to_string())
            .unwrap_or_else(|| candidate.filename.clone());

        let path = file
            .map(|f| f.fingerprint.as_str().to_string())
            .unwrap_or_else(|| candidate.path.clone());

        let (byte_range, line_range, page_range, content) = match chunk {
            Some(c) => (c.byte_range, c.line_range, c.page_range, c.content.clone()),
            None => {
                // If chunk is not preloaded, attempt db lookup if chunk_id is present
                if let Some(ref c_id) = candidate.chunk_id {
                    if let Ok(Some(loaded)) = db.get_chunk(c_id).await {
                        (
                            loaded.byte_range,
                            loaded.line_range,
                            loaded.page_range,
                            loaded.content,
                        )
                    } else {
                        (
                            ByteRange::new(0, candidate.snippet.len() as u64),
                            None,
                            None,
                            candidate.snippet.clone(),
                        )
                    }
                } else {
                    (
                        ByteRange::new(0, candidate.snippet.len() as u64),
                        None,
                        None,
                        candidate.snippet.clone(),
                    )
                }
            }
        };

        let locator_summary = Self::format_locator_summary(&filename, line_range, page_range);

        EvidenceLookup {
            chunk_id: candidate.chunk_id,
            file_id: candidate.file_id,
            filename,
            path,
            byte_range,
            line_range,
            page_range,
            snippet: candidate.snippet.clone(),
            locator_summary,
            content,
        }
    }
}
