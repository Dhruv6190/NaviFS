//! Repository for recording and querying temporal file lifecycle events (§13 & §17.1)

use chrono::{DateTime, Utc};
use navifs_core::{FileId, FileTemporalEvent, NaviError, Result};
use rusqlite::{params, Connection};

pub struct EventRepository;

impl EventRepository {
    /// Inserts a temporal file event into the database using parametric SQL
    pub fn record(conn: &Connection, event: &FileTemporalEvent) -> Result<()> {
        conn.execute(
            r#"
            INSERT INTO file_events (
                id, file_id, event_type, timestamp, source, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                event.id,
                event.file_id.to_string(),
                event.event_type,
                event.timestamp.to_rfc3339(),
                event.source,
                event.metadata,
            ],
        )
        .map_err(|e| NaviError::Database(format!("EventRepository::record failed: {}", e)))?;

        Ok(())
    }

    /// Retrieves recent temporal events for a specific file ordered newest first
    pub fn get_for_file(
        conn: &Connection,
        file_id: &FileId,
        limit: usize,
    ) -> Result<Vec<FileTemporalEvent>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, file_id, event_type, timestamp, source, metadata
                FROM file_events
                WHERE file_id = ?1
                ORDER BY timestamp DESC
                LIMIT ?2
                "#,
            )
            .map_err(|e| {
                NaviError::Database(format!(
                    "EventRepository::get_for_file prepare failed: {}",
                    e
                ))
            })?;

        let rows = stmt
            .query_map(params![file_id.to_string(), limit as i64], |row| {
                let id: String = row.get(0)?;
                let fid_str: String = row.get(1)?;
                let event_type: String = row.get(2)?;
                let ts_str: String = row.get(3)?;
                let source: String = row.get(4)?;
                let metadata: String = row.get(5)?;

                Ok((id, fid_str, event_type, ts_str, source, metadata))
            })
            .map_err(|e| {
                NaviError::Database(format!("EventRepository::get_for_file query failed: {}", e))
            })?;

        let mut events = Vec::new();
        for row in rows {
            let (id, fid_str, event_type, ts_str, source, metadata) =
                row.map_err(|e| NaviError::Database(format!("EventRepository row error: {}", e)))?;

            let parsed_fid = FileId::parse(&fid_str).map_err(|e| {
                NaviError::Database(format!("Invalid file_id in event record: {}", e))
            })?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            events.push(FileTemporalEvent {
                id,
                file_id: parsed_fid,
                event_type,
                timestamp,
                source,
                metadata,
            });
        }

        Ok(events)
    }
}
