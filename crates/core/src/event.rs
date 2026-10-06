use crate::identity::file::FileId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Classification of filesystem modifications
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed { old_path: PathBuf },
}

/// Raw filesystem event captured by watcher
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEvent {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub timestamp: DateTime<Utc>,
}

impl FsEvent {
    pub fn new(path: PathBuf, kind: ChangeKind) -> Self {
        Self {
            path,
            kind,
            timestamp: Utc::now(),
        }
    }
}

/// High-level event produced during indexing pipeline lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexEvent {
    FileDiscovered {
        file_id: FileId,
        path: PathBuf,
    },
    FileExtracted {
        file_id: FileId,
        chunks_count: usize,
        entities_count: usize,
    },
    FileIndexed {
        file_id: FileId,
        duration_ms: u64,
    },
    FileRemoved {
        file_id: FileId,
        path: PathBuf,
    },
    IndexFailed {
        path: PathBuf,
        error: String,
    },
}

/// Persistent temporal event record stored in SQLite for auditing and time-travel queries (§13 & §17.1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTemporalEvent {
    pub id: String,
    pub file_id: FileId,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub metadata: String,
}

impl FileTemporalEvent {
    pub fn new(
        file_id: FileId,
        event_type: impl Into<String>,
        source: impl Into<String>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            file_id,
            event_type: event_type.into(),
            timestamp: Utc::now(),
            source: source.into(),
            metadata: metadata
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string()),
        }
    }
}
