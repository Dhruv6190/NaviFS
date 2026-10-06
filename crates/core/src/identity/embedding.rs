use crate::identity::chunk::ChunkId;
use crate::identity::file::FileId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Strongly-typed identifier for embedding vectors
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EmbeddingId(Uuid);

impl EmbeddingId {
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

impl Default for EmbeddingId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EmbeddingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmbeddingId({})", self.0)
    }
}

impl fmt::Display for EmbeddingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Representation of a dense vector embedding stored for a file or chunk
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    pub id: EmbeddingId,
    pub file_id: FileId,
    pub chunk_id: Option<ChunkId>,
    pub model_name: String,
    pub dimensions: usize,
    pub vector: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

impl EmbeddingRecord {
    pub fn new(
        file_id: FileId,
        chunk_id: Option<ChunkId>,
        model_name: impl Into<String>,
        vector: Vec<f32>,
    ) -> Self {
        let dimensions = vector.len();
        Self {
            id: EmbeddingId::new(),
            file_id,
            chunk_id,
            model_name: model_name.into(),
            dimensions,
            vector,
            created_at: Utc::now(),
        }
    }

    /// Converts raw f32 vector into little-endian byte array for SQLite BLOB storage
    pub fn vector_to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vector.len() * 4);
        for &val in &self.vector {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }

    /// Reconstructs f32 vector from little-endian SQLite BLOB
    pub fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap_or([0; 4])))
            .collect()
    }

    /// Computes cosine similarity against another vector
    pub fn cosine_similarity(&self, query: &[f32]) -> f32 {
        if self.vector.is_empty() || query.is_empty() || self.vector.len() != query.len() {
            return 0.0;
        }

        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;

        for (a, b) in self.vector.iter().zip(query.iter()) {
            dot += a * b;
            norm_a += a * a;
            norm_b += b * b;
        }

        let denom = norm_a.sqrt() * norm_b.sqrt();
        if denom == 0.0 {
            0.0
        } else {
            dot / denom
        }
    }
}
