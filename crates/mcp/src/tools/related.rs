//! `related` tool: One-to-one hop graph relationship traversal discovering adjacent nodes and directional edges.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use navifs_core::{DatabaseStore, EntityId, EntityNode, FileId, RelationId, RelationType};
use navifs_graph::KnowledgeGraph;

#[derive(Debug, Deserialize)]
pub struct RelatedArgs {
    pub entity_id: Option<String>,
    pub file_id: Option<String>,
    pub path: Option<String>,
    pub relation_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectedEntity {
    pub edge_id: RelationId,
    pub relation_type: RelationType,
    pub weight: f32,
    pub properties: Value,
    pub entity: Option<EntityNode>,
    pub entity_id: EntityId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OneHopGraphResponse {
    pub center_entity: EntityNode,
    pub outgoing: Vec<ConnectedEntity>,
    pub incoming: Vec<ConnectedEntity>,
    pub total_connections: usize,
}

pub struct RelatedTool;

impl RelatedTool {
    pub fn schema() -> Value {
        serde_json::json!({
            "name": "related",
            "description": "Traverse one-to-one hop knowledge graph relationships for an entity, file, or heading to discover adjacent connected nodes and directional relation edges",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": {
                        "type": "string",
                        "description": "UUID of the graph entity"
                    },
                    "file_id": {
                        "type": "string",
                        "description": "UUID of the file to query related entities for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File path to query related entities for"
                    },
                    "relation_type": {
                        "type": "string",
                        "description": "Optional filter for specific relation type (e.g. 'Contains', 'References', 'Imports', 'Defines', 'ParentOf', 'ChildOf')"
                    }
                }
            }
        })
    }

    pub async fn execute(
        db: &dyn DatabaseStore,
        graph: &KnowledgeGraph,
        args: RelatedArgs,
    ) -> Result<OneHopGraphResponse, String> {
        // 1. Resolve Center Entity Node
        let center_entity = if let Some(ref eid_str) = args.entity_id {
            let eid = EntityId::parse(eid_str).map_err(|e| format!("Invalid entity_id: {}", e))?;
            if let Some(n) = graph.get_node(&eid).await {
                n
            } else if let Ok(Some(n)) = db.get_entity(&eid).await {
                n
            } else {
                return Err(format!("Entity with id '{}' not found", eid_str));
            }
        } else if let Some(ref fid_str) = args.file_id {
            let fid = FileId::parse(fid_str).map_err(|e| format!("Invalid file_id: {}", e))?;
            let entities = db.get_entities_for_file(&fid).await.map_err(|e| e.to_string())?;
            entities.into_iter().next().ok_or_else(|| {
                format!("No entities found for file_id '{}'", fid_str)
            })?
        } else if let Some(ref path_str) = args.path {
            let file = db
                .get_file_by_path(path_str)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("File with path '{}' not found", path_str))?;

            let entities = db.get_entities_for_file(&file.id).await.map_err(|e| e.to_string())?;
            entities.into_iter().next().ok_or_else(|| {
                format!("No entities found for file '{}'", path_str)
            })?
        } else {
            return Err("Must provide either 'entity_id', 'file_id', or 'path'".to_string());
        };

        // 2. Retrieve Relationships from In-Memory Graph and Database Store
        let center_id = center_entity.id;
        let mut edges = db
            .get_relations_for_entity(&center_id)
            .await
            .unwrap_or_default();

        // Also merge any in-memory graph edges
        let mem_outgoing = graph.get_outgoing_edges(&center_id).await;
        let mem_incoming = graph.get_incoming_edges(&center_id).await;
        for edge in mem_outgoing.into_iter().chain(mem_incoming) {
            if !edges.iter().any(|e| e.id == edge.id) {
                edges.push(edge);
            }
        }

        // 3. Optional filter by relation_type
        if let Some(ref rel_filter) = args.relation_type {
            edges.retain(|e| {
                format!("{:?}", e.relation_type).eq_ignore_ascii_case(rel_filter)
            });
        }

        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();

        for edge in edges {
            if edge.source_id == center_id {
                let target_node = if let Some(n) = graph.get_node(&edge.target_id).await {
                    Some(n)
                } else {
                    db.get_entity(&edge.target_id).await.unwrap_or(None)
                };

                outgoing.push(ConnectedEntity {
                    edge_id: edge.id,
                    relation_type: edge.relation_type,
                    weight: edge.weight,
                    properties: edge.properties,
                    entity: target_node,
                    entity_id: edge.target_id,
                });
            } else if edge.target_id == center_id {
                let source_node = if let Some(n) = graph.get_node(&edge.source_id).await {
                    Some(n)
                } else {
                    db.get_entity(&edge.source_id).await.unwrap_or(None)
                };

                incoming.push(ConnectedEntity {
                    edge_id: edge.id,
                    relation_type: edge.relation_type,
                    weight: edge.weight,
                    properties: edge.properties,
                    entity: source_node,
                    entity_id: edge.source_id,
                });
            }
        }

        let total_connections = outgoing.len() + incoming.len();

        Ok(OneHopGraphResponse {
            center_entity,
            outgoing,
            incoming,
            total_connections,
        })
    }
}
