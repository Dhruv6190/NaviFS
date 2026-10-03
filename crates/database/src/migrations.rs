//! Database migration engine executing embedded SQL scripts in atomic transactions

use chrono::Utc;
use rusqlite::{params, Connection};
use tracing::{info, warn};
use navifs_core::{NaviError, Result};

pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_initial_schema",
        sql: include_str!("../migrations/0001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "0002_fts5_search",
        sql: include_str!("../migrations/0002_fts5_search.sql"),
    },
];

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    // Ensure migrations tracking table exists
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );
        "#,
    )
    .map_err(|e| NaviError::Database(format!("Failed to create schema_migrations table: {}", e)))?;

    for migration in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                params![migration.version],
                |row| row.get(0),
            )
            .map_err(|e| NaviError::Database(format!("Failed to check migration {}: {}", migration.name, e)))?;

        if !already_applied {
            info!("Applying database migration #{}: {}", migration.version, migration.name);

            let tx = conn
                .transaction()
                .map_err(|e| NaviError::Database(format!("Failed to begin migration transaction: {}", e)))?;

            tx.execute_batch(migration.sql)
                .map_err(|e| NaviError::Database(format!("Failed executing migration {}: {}", migration.name, e)))?;

            let now = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, migration.name, now],
            )
            .map_err(|e| NaviError::Database(format!("Failed recording migration {}: {}", migration.name, e)))?;

            tx.commit()
                .map_err(|e| NaviError::Database(format!("Failed committing migration {}: {}", migration.name, e)))?;

            info!("Migration #{} successfully applied", migration.version);
        }
    }

    Ok(())
}
