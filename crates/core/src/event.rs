use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::identity::file::FileId;

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
