use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, NaviError>;

#[derive(Error, Debug)]
pub enum NaviError {
    #[error("I/O error at {path}: {source}")]
    IoWithPath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Standard I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization/Deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Extraction failed for file {path}: {reason}")]
    ExtractionError { path: PathBuf, reason: String },

    #[error("Indexing error: {0}")]
    IndexingError(String),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Graph error: {0}")]
    GraphError(String),

    #[error("MCP protocol error ({code}): {message}")]
    McpError { code: i32, message: String },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Invalid identifier: {0}")]
    InvalidId(String),

    #[error("Operation cancelled or timed out: {0}")]
    Cancelled(String),

    #[error("Internal engine error: {0}")]
    Internal(String),
}
