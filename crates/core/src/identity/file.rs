use crate::foundation::{ContentHash, MimeType, PathFingerprint};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use uuid::Uuid;

/// Strongly-typed unique identifier for files registered in NaviFS
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileId(Uuid);

impl FileId {
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

impl Default for FileId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Processing lifecycle status of a file within NaviFS
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Discovered,
    Indexing,
    Indexed,
    Modified,
    Deleted,
    Error(String),
}

/// Core File Identity: The definitive structural identity model representing
/// a physical or logical file tracked by the NaviFS engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub id: FileId,
    pub fingerprint: PathFingerprint,
    pub mime_type: MimeType,
    pub size_bytes: u64,
    pub content_hash: Option<ContentHash>,
    pub status: FileStatus,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub indexed_at: Option<DateTime<Utc>>,
    /// Windows NTFS file index or POSIX inode for rename tracking
    pub file_index: Option<u64>,
    /// Device / volume identifier
    pub device_id: Option<u64>,
}

impl FileIdentity {
    pub fn new(path: &Path, size_bytes: u64, modified_at: DateTime<Utc>) -> Self {
        let fingerprint = PathFingerprint::from_path(path);
        let mime_type = match fingerprint.extension() {
            Some(ext) => MimeType::from_extension(ext),
            None => MimeType::new("application/octet-stream"),
        };

        Self {
            id: FileId::new(),
            fingerprint,
            mime_type,
            size_bytes,
            content_hash: None,
            status: FileStatus::Discovered,
            created_at: Utc::now(),
            modified_at,
            indexed_at: None,
            file_index: None,
            device_id: None,
        }
    }

    pub fn with_hash(mut self, hash: ContentHash) -> Self {
        self.content_hash = Some(hash);
        self
    }

    pub fn mark_indexed(&mut self) {
        self.status = FileStatus::Indexed;
        self.indexed_at = Some(Utc::now());
    }

    pub fn mark_modified(&mut self, new_size: u64, modified_at: DateTime<Utc>) {
        self.size_bytes = new_size;
        self.modified_at = modified_at;
        self.content_hash = None;
        self.status = FileStatus::Modified;
    }

    pub fn mark_deleted(&mut self) {
        self.status = FileStatus::Deleted;
    }

    pub fn mark_error(&mut self, error: impl Into<String>) {
        self.status = FileStatus::Error(error.into());
    }
}
