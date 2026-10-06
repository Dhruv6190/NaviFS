use crate::error::{NaviError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Strongly typed cryptographic content hash (SHA-256)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(hash_hex: impl Into<String>) -> Self {
        Self(hash_hex.into().to_lowercase())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        Self(hex::encode(result))
    }

    pub fn from_reader<R: Read>(mut reader: R) -> Result<Self> {
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = reader.read(&mut buffer).map_err(NaviError::Io)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        let result = hasher.finalize();
        Ok(Self(hex::encode(result)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", &self.0[..8.min(self.0.len())])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Normalized, cross-platform path fingerprint
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PathFingerprint {
    normalized_path: String,
    filename: String,
    extension: Option<String>,
}

impl PathFingerprint {
    pub fn from_path(path: &Path) -> Self {
        let normalized = path
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();

        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase());

        Self {
            normalized_path: normalized,
            filename,
            extension,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.normalized_path
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.normalized_path)
    }
}

impl fmt::Display for PathFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.normalized_path)
    }
}

/// MIME Type categorization for intelligent extraction pipelines
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MimeType(String);

impl MimeType {
    pub fn new(mime: impl Into<String>) -> Self {
        Self(mime.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_text(&self) -> bool {
        self.0.starts_with("text/")
            || self.0 == "application/json"
            || self.0 == "application/xml"
            || self.0 == "application/javascript"
    }

    pub fn is_markdown(&self) -> bool {
        self.0 == "text/markdown" || self.0 == "text/x-markdown"
    }

    pub fn is_code(&self) -> bool {
        self.0.starts_with("text/x-")
            || self.0 == "application/javascript"
            || self.0 == "application/typescript"
            || self.0 == "application/x-rust"
            || self.0 == "application/x-python"
    }

    pub fn is_binary(&self) -> bool {
        !self.is_text()
    }

    pub fn is_spreadsheet(&self) -> bool {
        self.0 == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            || self.0 == "application/vnd.ms-excel"
            || self.0 == "application/vnd.oasis.opendocument.spreadsheet"
    }

    pub fn is_docx(&self) -> bool {
        self.0 == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            || self.0 == "application/msword"
    }

    /// Fast extension-based MIME guesser
    pub fn from_extension(ext: &str) -> Self {
        let mime = match ext.to_lowercase().as_str() {
            "rs" => "application/x-rust",
            "py" => "application/x-python",
            "js" => "application/javascript",
            "ts" => "application/typescript",
            "tsx" | "jsx" => "text/jsx",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "json" => "application/json",
            "toml" => "text/x-toml",
            "yaml" | "yml" => "text/yaml",
            "md" | "markdown" => "text/markdown",
            "txt" | "log" => "text/plain",
            "csv" => "text/csv",
            "pdf" => "application/pdf",
            "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xls" => "application/vnd.ms-excel",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "doc" => "application/msword",
            "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "svg" => "image/svg+xml",
            _ => "application/octet-stream",
        };
        Self(mime.to_string())
    }
}

impl fmt::Display for MimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Zero-indexed byte range within a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// 1-indexed line range within a text document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start_line: usize,
    pub end_line: usize,
}

impl LineRange {
    pub fn new(start_line: usize, end_line: usize) -> Self {
        Self {
            start_line,
            end_line,
        }
    }

    pub fn line_count(&self) -> usize {
        if self.end_line >= self.start_line {
            self.end_line - self.start_line + 1
        } else {
            0
        }
    }
}

/// 1-indexed page range within a paginated document (e.g., PDF)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRange {
    pub start_page: usize,
    pub end_page: usize,
}

impl PageRange {
    pub fn new(start_page: usize, end_page: usize) -> Self {
        Self {
            start_page,
            end_page,
        }
    }

    pub fn single(page: usize) -> Self {
        Self {
            start_page: page,
            end_page: page,
        }
    }

    pub fn page_count(&self) -> usize {
        if self.end_page >= self.start_page {
            self.end_page - self.start_page + 1
        } else {
            0
        }
    }
}

/// Unified index locator capturing byte, line, and page bounds
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexLocator {
    pub byte_range: ByteRange,
    pub line_range: Option<LineRange>,
    pub page_range: Option<PageRange>,
}

impl IndexLocator {
    pub fn new(byte_range: ByteRange) -> Self {
        Self {
            byte_range,
            line_range: None,
            page_range: None,
        }
    }

    pub fn with_lines(mut self, start_line: usize, end_line: usize) -> Self {
        self.line_range = Some(LineRange::new(start_line, end_line));
        self
    }

    pub fn with_pages(mut self, start_page: usize, end_page: usize) -> Self {
        self.page_range = Some(PageRange::new(start_page, end_page));
        self
    }
}
