//! Extractor registry dispatching files to format-specific document extractors

use std::path::Path;
use std::sync::Arc;
use tracing::debug;
use navifs_core::{DocumentExtractor, ExtractionOutput, FileIdentity, Result};
use crate::docx::DocxExtractor;
use crate::json::JsonExtractor;
use crate::markdown::MarkdownExtractor;
use crate::pdf::PdfExtractor;
use crate::plain_text::PlainTextExtractor;
use crate::xlsx::XlsxExtractor;

/// Registry managing active document extractors
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
            Arc::new(XlsxExtractor),
            Arc::new(DocxExtractor),
            Arc::new(PdfExtractor),
            Arc::new(MarkdownExtractor),
            Arc::new(JsonExtractor),
            fallback.clone(),
        ];

        Self {
            extractors,
            fallback,
        }
    }

    /// Adds a new extractor with highest precedence
    pub fn register(&mut self, extractor: Arc<dyn DocumentExtractor>) {
        self.extractors.insert(0, extractor);
    }

    /// Selects the best supported extractor and processes the file
    pub async fn extract_file(
        &self,
        identity: &FileIdentity,
        path: &Path,
    ) -> Result<ExtractionOutput> {
        for ext in &self.extractors {
            if ext.supports(identity) {
                debug!(
                    "Dispatching {:?} to extractor [{}]",
                    identity.fingerprint.as_str(),
                    ext.name()
                );
                return ext.extract(identity, path).await;
            }
        }

        self.fallback.extract(identity, path).await
    }
}
