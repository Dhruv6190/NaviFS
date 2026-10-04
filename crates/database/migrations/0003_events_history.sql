-- NaviFS Migration 0003: Temporal Event History (§13 & §17.1)
-- Tracks file creation, modification, re-indexing, deletion, and rename lifecycle events.

CREATE TABLE IF NOT EXISTS file_events (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    source TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_file_events_file_id ON file_events (file_id);
CREATE INDEX IF NOT EXISTS idx_file_events_timestamp ON file_events (timestamp);
