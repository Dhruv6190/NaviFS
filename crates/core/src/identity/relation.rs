use std::fmt;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::identity::entity::EntityId;

/// Strongly typed relation identifier
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RelationId(Uuid);

impl RelationId {
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

impl Default for RelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelationId({})", self.0)
    }
}

impl fmt::Display for RelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Relationship edge semantics between entities
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    Contains,
    Imports,
    References,
    Defines,
    Implements,
    Calls,
    ChildOf,
    SimilarTo,
    DependsOn,
    Custom(String),
}

/// Directed edge connecting two entity nodes in the NaviFS knowledge graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationEdge {
    pub id: RelationId,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: f32,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl RelationEdge {
    pub fn new(source_id: EntityId, target_id: EntityId, relation_type: RelationType) -> Self {
        Self {
            id: RelationId::new(),
            source_id,
            target_id,
            relation_type,
            weight: 1.0,
            properties: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = properties;
        self
    }
}
