-- NaviFS Initial Database Migration: Core Tables
-- Creates tables for files, content_chunks (contentions), embeddings, and relationships.

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);

-- Files Table: Primary catalog of all monitored and indexed files
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    content_hash TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    indexed_at TEXT,
    file_index INTEGER,
    device_id INTEGER
);

-- Content Chunks Table: Extracted granular content sections (paragraphs, ast code blocks, headings)
CREATE TABLE IF NOT EXISTS content_chunks (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    chunk_type TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    line_start INTEGER,
    line_end INTEGER,
    page_start INTEGER,
    page_end INTEGER,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE CASCADE
);

-- Embeddings Table: Vector embeddings for semantic search over chunks and documents
CREATE TABLE IF NOT EXISTS embeddings (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL,
    chunk_id TEXT,
    model_name TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    vector BLOB NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE CASCADE,
    FOREIGN KEY (chunk_id) REFERENCES content_chunks (id) ON DELETE CASCADE
);

-- Entities Table: Extracted knowledge graph nodes (functions, classes, documents, sections)
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    file_id TEXT,
    chunk_id TEXT,
    properties TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files (id) ON DELETE SET NULL,
    FOREIGN KEY (chunk_id) REFERENCES content_chunks (id) ON DELETE SET NULL
);

-- Relationships Table: Graph edges between files and entities (contains, imports, calls, references)
CREATE TABLE IF NOT EXISTS relationships (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    properties TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

-- Indices for rapid querying and foreign key performance
CREATE INDEX IF NOT EXISTS idx_files_path ON files (path);
CREATE INDEX IF NOT EXISTS idx_files_hash ON files (content_hash);
CREATE INDEX IF NOT EXISTS idx_files_modified ON files (modified_at);
CREATE INDEX IF NOT EXISTS idx_chunks_file_id ON content_chunks (file_id);
CREATE INDEX IF NOT EXISTS idx_chunks_file_idx ON content_chunks (file_id, chunk_index);
CREATE INDEX IF NOT EXISTS idx_embeddings_file ON embeddings (file_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk ON embeddings (chunk_id);
CREATE INDEX IF NOT EXISTS idx_entities_name ON entities (name);
CREATE INDEX IF NOT EXISTS idx_entities_file ON entities (file_id);
CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships (source_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships (target_id);
CREATE INDEX IF NOT EXISTS idx_relationships_type ON relationships (relation_type);
