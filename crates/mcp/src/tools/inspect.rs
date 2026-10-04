//! `inspect` tool: Generates structural metadata summaries, chunk statistics, and outline entities.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use navifs_core::{DatabaseStore, EntityId, EntityType, FileId};

#[derive(Debug, Deserialize)]
pub struct InspectArgs {
    pub file_id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: EntityId,
    pub name: String,
    pub entity_type: EntityType,
    pub properties: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentStructure {
    pub kind: String,
    pub sheets: Option<Vec<String>>,
    pub sections: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEventSummary {
    pub id: String,
    pub event_type: String,
    pub timestamp: String,
    pub source: String,
    pub metadata: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataSummary {
    pub file_id: FileId,
    pub path: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub content_hash: Option<String>,
    pub status: String,
    pub created_at: String,
    pub modified_at: String,
    pub indexed_at: Option<String>,
    pub chunk_count: usize,
    pub page_count: Option<usize>,
    pub line_count: Option<usize>,
    pub outline: Vec<EntitySummary>,
    pub structure: Option<DocumentStructure>,
    pub recent_events: Vec<FileEventSummary>,
}

pub struct InspectTool;

impl InspectTool {
    pub fn schema() -> Value {
        serde_json::json!({
            "name": "inspect",
            "description": "Inspect file metadata summaries, structural attributes, chunk statistics, MIME types, content hashes, and outline entities for a given file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_id": {
                        "type": "string",
                        "description": "UUID of the file to inspect"
                    },
                    "path": {
                        "type": "string",
                        "description": "Filesystem path of the file to inspect"
                    }
                }
            }
        })
    }

    pub async fn execute(
        db: &dyn DatabaseStore,
        args: InspectArgs,
    ) -> Result<MetadataSummary, String> {
        let file = if let Some(ref id_str) = args.file_id {
            let fid = FileId::parse(id_str).map_err(|e| format!("Invalid file_id: {}", e))?;
            db.get_file(&fid)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("File with id '{}' not found", id_str))?
        } else if let Some(ref path_str) = args.path {
            db.get_file_by_path(path_str)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("File with path '{}' not found in database", path_str))?
        } else {
            return Err("Either 'file_id' or 'path' must be provided to inspect".to_string());
        };

        // 1. Retrieve chunks to compute chunk and page metrics
        let chunks = db
            .get_chunks_for_file(&file.id)
            .await
            .unwrap_or_default();

        let chunk_count = chunks.len();
        let mut max_page: Option<usize> = None;
        let mut max_line: Option<usize> = None;

        for chunk in &chunks {
            if let Some(ref page) = chunk.page_range {
                max_page = Some(max_page.map_or(page.end_page, |p| p.max(page.end_page)));
            }
            if let Some(ref line) = chunk.line_range {
                max_line = Some(max_line.map_or(line.end_line, |l| l.max(line.end_line)));
            }
        }

        // 2. Retrieve document outline entities (headings, sections, functions)
        let entities = db
            .get_entities_for_file(&file.id)
            .await
            .unwrap_or_default();

        // 3. Compute structural taxonomy (§7.4)
        let structure = if file.mime_type.is_spreadsheet() {
            let sheets: Vec<String> = entities
                .iter()
                .filter(|e| e.properties.get("sheet").is_some() || e.entity_type == EntityType::Heading)
                .map(|e| e.name.clone())
                .collect();
            Some(DocumentStructure {
                kind: "spreadsheet".to_string(),
                sheets: Some(sheets),
                sections: None,
            })
        } else if file.mime_type.is_docx() || file.mime_type.is_markdown() {
            let sections: Vec<String> = entities
                .iter()
                .filter(|e| e.entity_type == EntityType::Heading)
                .map(|e| e.name.clone())
                .collect();
            Some(DocumentStructure {
                kind: "document".to_string(),
                sheets: None,
                sections: Some(sections),
            })
        } else if file.mime_type.is_code() {
            let symbols: Vec<String> = entities
                .iter()
                .map(|e| format!("{:?}: {}", e.entity_type, e.name))
                .collect();
            Some(DocumentStructure {
                kind: "code".to_string(),
                sheets: None,
                sections: Some(symbols),
            })
        } else {
            None
        };

        let outline = entities
            .into_iter()
            .map(|e| EntitySummary {
                id: e.id,
                name: e.name,
                entity_type: e.entity_type,
                properties: e.properties,
            })
            .collect();

        // 4. Retrieve recent temporal lifecycle events (§13 & §17.1)
        let raw_events = db.get_events_for_file(&file.id, 10).await.unwrap_or_default();
        let recent_events = raw_events
            .into_iter()
            .map(|ev| {
                let metadata: Value = serde_json::from_str(&ev.metadata)
                    .unwrap_or_else(|_| serde_json::json!({}));
                FileEventSummary {
                    id: ev.id,
                    event_type: ev.event_type,
                    timestamp: ev.timestamp.to_rfc3339(),
                    source: ev.source,
                    metadata,
                }
            })
            .collect();

        Ok(MetadataSummary {
            file_id: file.id,
            path: file.fingerprint.as_str().to_string(),
            filename: file.fingerprint.filename().to_string(),
            mime_type: file.mime_type.as_str().to_string(),
            size_bytes: file.size_bytes,
            content_hash: file.content_hash.map(|h| h.as_str().to_string()),
            status: format!("{:?}", file.status),
            created_at: file.created_at.to_rfc3339(),
            modified_at: file.modified_at.to_rfc3339(),
            indexed_at: file.indexed_at.map(|t| t.to_rfc3339()),
            chunk_count,
            page_count: max_page,
            line_count: max_line,
            outline,
            structure,
            recent_events,
        })
    }
}
