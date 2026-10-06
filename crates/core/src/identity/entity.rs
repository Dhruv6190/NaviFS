use crate::identity::chunk::ChunkId;
use crate::identity::file::FileId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Strongly-typed identifier for graph entities
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(Uuid);

impl EntityId {
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

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Categorical types for entities tracked in the NaviFS knowledge graph
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    File,
    Directory,
    Function,
    ClassOrStruct,
    TraitOrInterface,
    Module,
    Heading,
    Section,
    Document,
    Topic,
    Tag,
    Custom(String),
}

/// Graph Node: Represents a knowledge entity derived from files, code, or documents
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityNode {
    pub id: EntityId,
    pub name: String,
    pub entity_type: EntityType,
    pub file_id: Option<FileId>,
    pub chunk_id: Option<ChunkId>,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl EntityNode {
    pub fn new(name: impl Into<String>, entity_type: EntityType) -> Self {
        Self {
            id: EntityId::new(),
            name: name.into(),
            entity_type,
            file_id: None,
            chunk_id: None,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }

    pub fn with_file_id(mut self, file_id: FileId) -> Self {
        self.file_id = Some(file_id);
        self
    }

    pub fn with_chunk_id(mut self, chunk_id: ChunkId) -> Self {
        self.chunk_id = Some(chunk_id);
        self
    }

    pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = properties;
        self
    }
}
