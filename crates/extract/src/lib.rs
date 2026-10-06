//! Multi-modal extraction subsystem for NaviFS
//!
//! Provides content parsing, semantic windowed chunking, index locators (with byte, line,
//! and page bounds), and structural entity extraction for plain-text, markdown, JSON, and PDF documents.

pub mod docx;
pub mod json;
pub mod markdown;
pub mod pdf;
pub mod plain_text;
pub mod registry;
pub mod xlsx;

pub use docx::DocxExtractor;
pub use json::JsonExtractor;
pub use markdown::MarkdownExtractor;
pub use pdf::PdfExtractor;
pub use plain_text::PlainTextExtractor;
pub use registry::ExtractorRegistry;
pub use xlsx::XlsxExtractor;

#[cfg(test)]
mod tests {
    use super::*;
    use navifs_core::{DocumentExtractor, FileIdentity};

    #[tokio::test]
    async fn test_plain_text_locator_bounds() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("navifs_test_text.txt");
        let content = "Line 1: Alpha\nLine 2: Beta\nLine 3: Gamma\nLine 4: Delta";
        std::fs::write(&test_file, content).unwrap();

        let identity = FileIdentity::new(&test_file, content.len() as u64, chrono::Utc::now());
        let extractor = PlainTextExtractor::new(20, 5);
        let result = extractor.extract(&identity, &test_file).await.unwrap();

        assert!(!result.chunks.is_empty());
        let first_chunk = &result.chunks[0];
        let locator = first_chunk.locator();
        assert!(locator.line_range.is_some());
        assert_eq!(locator.line_range.unwrap().start_line, 1);

        let _ = std::fs::remove_file(test_file);
    }

    #[tokio::test]
    async fn test_markdown_heading_extraction() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("navifs_test_doc.md");
        let content = "# Project Architecture\n\nOverview text\n\n## Database\n\nSQLite details";
        std::fs::write(&test_file, content).unwrap();

        let identity = FileIdentity::new(&test_file, content.len() as u64, chrono::Utc::now());
        let extractor = MarkdownExtractor;
        let result = extractor.extract(&identity, &test_file).await.unwrap();

        assert_eq!(result.chunks.len(), 2);
        assert_eq!(result.entities.len(), 3); // 1 document + 2 headings
        assert_eq!(result.relations.len(), 2);

        let _ = std::fs::remove_file(test_file);
    }

    #[tokio::test]
    async fn test_pdf_stream_text_and_bounds() {
        let raw_stream =
            b"BT /F1 12 Tf (NaviFS Intelligent System) Tj T* (Page Content Line 2) Tj ET";
        let lines = PdfExtractor::extract_text_from_stream(raw_stream);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "NaviFS Intelligent System");
        assert_eq!(lines[1], "Page Content Line 2");
    }

    #[test]
    fn test_xlsx_mime_support() {
        let path = std::path::Path::new("C:/test/leads.xlsx");
        let identity = FileIdentity::new(path, 1024, chrono::Utc::now());
        let extractor = XlsxExtractor;
        assert!(extractor.supports(&identity));
    }

    #[test]
    fn test_docx_mime_support() {
        let path = std::path::Path::new("C:/test/report.docx");
        let identity = FileIdentity::new(path, 2048, chrono::Utc::now());
        let extractor = DocxExtractor;
        assert!(extractor.supports(&identity));
    }
}
