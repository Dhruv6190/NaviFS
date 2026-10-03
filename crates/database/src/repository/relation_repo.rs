//! Repository for persisting graph entities and relationships using parametric queries

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use navifs_core::{
    ChunkId, EntityId, EntityNode, EntityType, FileId, NaviError, RelationEdge, RelationId,
    RelationType, Result,
};

pub struct RelationRepository;

impl RelationRepository {
    /// Inserts or updates relationships inside a transaction with parametric inputs
    pub fn save_relationships(conn: &mut Connection, relations: &[RelationEdge]) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| NaviError::Database(format!("Failed to begin relations transaction: {}", e)))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    r#"
                    INSERT INTO relationships (
                        id, source_id, target_id, relation_type, weight, properties, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ON CONFLICT(id) DO UPDATE SET
                        relation_type = excluded.relation_type,
                        weight = excluded.weight,
                        properties = excluded.properties
                    "#,
                )
                .map_err(|e| NaviError::Database(e.to_string()))?;

            for r in relations {
                let r_type_str = serde_json::to_string(&r.relation_type)?;
                let props_str = serde_json::to_string(&r.properties)?;
                stmt.execute(params![
                    r.id.to_string(),
                    r.source_id.to_string(),
                    r.target_id.to_string(),
                    r_type_str,
                    r.weight as f64,
                    props_str,
                    r.created_at.to_rfc3339(),
                ])
                .map_err(|e| NaviError::Database(format!("Failed inserting relationship: {}", e)))?;
            }
        }

        tx.commit()
            .map_err(|e| NaviError::Database(format!("Failed committing relations transaction: {}", e)))?;

        Ok(())
    }

    /// Fetches all relationships where entity_id is either source or target
    pub fn get_relationships(conn: &Connection, entity_id: &EntityId) -> Result<Vec<RelationEdge>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, source_id, target_id, relation_type, weight, properties, created_at
                FROM relationships WHERE source_id = ?1 OR target_id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let id_str = entity_id.to_string();
        let rows = stmt
            .query_map(params![id_str], |row| {
                let id_s: String = row.get(0)?;
                let src_s: String = row.get(1)?;
                let tgt_s: String = row.get(2)?;
                let t_s: String = row.get(3)?;
                let w: f64 = row.get(4)?;
                let p_s: String = row.get(5)?;
                let cr: String = row.get(6)?;

                let id = RelationId::parse(&id_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let source_id = EntityId::parse(&src_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let target_id = EntityId::parse(&tgt_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let relation_type: RelationType = serde_json::from_str(&t_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let properties: serde_json::Value = serde_json::from_str(&p_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let created_at = DateTime::parse_from_rfc3339(&cr)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?
                    .with_timezone(&Utc);

                Ok(RelationEdge {
                    id,
                    source_id,
                    target_id,
                    relation_type,
                    weight: w as f32,
                    properties,
                    created_at,
                })
            })
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut edges = Vec::new();
        for edge_res in rows {
            edges.push(edge_res.map_err(|e| NaviError::Database(e.to_string()))?);
        }
        Ok(edges)
    }

    /// Inserts or updates entities in batch inside a transaction with parametric inputs
    pub fn save_entities(conn: &mut Connection, entities: &[EntityNode]) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| NaviError::Database(format!("Failed to begin entities transaction: {}", e)))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    r#"
                    INSERT INTO entities (
                        id, name, entity_type, file_id, chunk_id, properties, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ON CONFLICT(id) DO UPDATE SET
                        name = excluded.name,
                        entity_type = excluded.entity_type,
                        properties = excluded.properties
                    "#,
                )
                .map_err(|e| NaviError::Database(e.to_string()))?;

            for e in entities {
                let e_type_str = serde_json::to_string(&e.entity_type)?;
                let props_str = serde_json::to_string(&e.properties)?;
                stmt.execute(params![
                    e.id.to_string(),
                    e.name,
                    e_type_str,
                    e.file_id.map(|id| id.to_string()),
                    e.chunk_id.map(|id| id.to_string()),
                    props_str,
                    e.created_at.to_rfc3339(),
                ])
                .map_err(|err| NaviError::Database(format!("Failed inserting entity: {}", err)))?;
            }
        }

        tx.commit()
            .map_err(|e| NaviError::Database(format!("Failed committing entities transaction: {}", e)))?;

        Ok(())
    }

    /// Fetches an entity by EntityId
    pub fn get_entity(conn: &Connection, id: &EntityId) -> Result<Option<EntityNode>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, name, entity_type, file_id, chunk_id, properties, created_at
                FROM entities WHERE id = ?1
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let id_str = id.to_string();
        let entity = stmt
            .query_row(params![id_str], |row| {
                let id_s: String = row.get(0)?;
                let name: String = row.get(1)?;
                let t_s: String = row.get(2)?;
                let f_s: Option<String> = row.get(3)?;
                let c_s: Option<String> = row.get(4)?;
                let p_s: String = row.get(5)?;
                let cr: String = row.get(6)?;

                let node_id = EntityId::parse(&id_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let entity_type: EntityType = serde_json::from_str(&t_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let properties: serde_json::Value = serde_json::from_str(&p_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let file_id = f_s.and_then(|s| FileId::parse(&s).ok());
                let chunk_id = c_s.and_then(|s| ChunkId::parse(&s).ok());
                let created_at = DateTime::parse_from_rfc3339(&cr)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?
                    .with_timezone(&Utc);

                Ok(EntityNode {
                    id: node_id,
                    name,
                    entity_type,
                    file_id,
                    chunk_id,
                    properties,
                    created_at,
                })
            })
            .optional()
            .map_err(|e| NaviError::Database(e.to_string()))?;

        Ok(entity)
    }

    /// Finds entities by name prefix or exact match using parametric queries
    pub fn find_entities_by_name(conn: &Connection, name_pattern: &str, limit: usize) -> Result<Vec<EntityNode>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, name, entity_type, file_id, chunk_id, properties, created_at
                FROM entities WHERE name LIKE ?1 LIMIT ?2
                "#,
            )
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let pattern = format!("%{}%", name_pattern);
        let rows = stmt
            .query_map(params![pattern, limit as i64], |row| {
                let id_s: String = row.get(0)?;
                let name: String = row.get(1)?;
                let t_s: String = row.get(2)?;
                let f_s: Option<String> = row.get(3)?;
                let c_s: Option<String> = row.get(4)?;
                let p_s: String = row.get(5)?;
                let cr: String = row.get(6)?;

                let node_id = EntityId::parse(&id_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let entity_type: EntityType = serde_json::from_str(&t_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let properties: serde_json::Value = serde_json::from_str(&p_s).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let file_id = f_s.and_then(|s| FileId::parse(&s).ok());
                let chunk_id = c_s.and_then(|s| ChunkId::parse(&s).ok());
                let created_at = DateTime::parse_from_rfc3339(&cr)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?
                    .with_timezone(&Utc);

                Ok(EntityNode {
                    id: node_id,
                    name,
                    entity_type,
                    file_id,
                    chunk_id,
                    properties,
                    created_at,
                })
            })
            .map_err(|e| NaviError::Database(e.to_string()))?;

        let mut nodes = Vec::new();
        for node_res in rows {
            nodes.push(node_res.map_err(|e| NaviError::Database(e.to_string()))?);
        }
        Ok(nodes)
    }
}
