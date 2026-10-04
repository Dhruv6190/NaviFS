use std::fmt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::foundation::{ByteRange, ContentHash, IndexLocator, LineRange, PageRange};
use crate::identity::file::FileId;

/// Strongly typed chunk identifier
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkId(Uuid);

impl ChunkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ChunkId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId({})", self.0)
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Semantic chunk type category
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    CodeBlock { language: String },
    MarkdownSection { heading: String, depth: u8 },
    DocxSection { heading: String, depth: u8 },
    TextParagraph,
    JsonBlock,
    PdfPageSection { page: usize, total_pages: Option<usize> },
    SpreadsheetSheet { sheet: String, row_start: usize, row_end: usize },
    CsvRowGroup,
    Generic,
}

/// Granular, indexable slice of file content for vector embeddings & lexical search
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChunk {
    pub id: ChunkId,
    pub file_id: FileId,
    pub chunk_index: u32,
    pub chunk_type: ChunkType,
    pub byte_range: ByteRange,
    pub line_range: Option<LineRange>,
    pub page_range: Option<PageRange>,
    pub content: String,
    pub token_count: usize,
    pub content_hash: ContentHash,
}

impl FileChunk {
    pub fn new(
        file_id: FileId,
        chunk_index: u32,
        chunk_type: ChunkType,
        byte_range: ByteRange,
        line_range: Option<LineRange>,
        content: String,
    ) -> Self {
        let content_hash = ContentHash::from_bytes(content.as_bytes());
        // Rough heuristic token count (~4 chars per token)
        let token_count = content.len() / 4;

        Self {
            id: ChunkId::new(),
            file_id,
            chunk_index,
            chunk_type,
            byte_range,
            line_range,
            page_range: None,
            content,
            token_count,
            content_hash,
        }
    }

    pub fn with_page_range(mut self, page_range: PageRange) -> Self {
        self.page_range = Some(page_range);
        self
    }

    pub fn with_locator(mut self, locator: IndexLocator) -> Self {
        self.byte_range = locator.byte_range;
        self.line_range = locator.line_range;
        self.page_range = locator.page_range;
        self
    }

    pub fn locator(&self) -> IndexLocator {
        IndexLocator {
            byte_range: self.byte_range,
            line_range: self.line_range,
            page_range: self.page_range,
        }
    }
}
