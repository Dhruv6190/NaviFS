-- NaviFS Migration 0002: SQLite FTS5 Full-Text Search Integration
-- Enables full-text lexical search across filenames, paths, and extracted text content with automated triggers.

CREATE VIRTUAL TABLE IF NOT EXISTS fts_content USING fts5(
    chunk_id UNINDEXED,
    file_id UNINDEXED,
    filename,
    path,
    content,
    tokenize = 'porter unicode61 remove_diacritics 1'
);

-- Trigger: Synchronize new chunks into FTS5 virtual table
CREATE TRIGGER IF NOT EXISTS trig_chunks_after_insert AFTER INSERT ON content_chunks
BEGIN
    INSERT INTO fts_content (chunk_id, file_id, filename, path, content)
    SELECT
        NEW.id,
        NEW.file_id,
        f.filename,
        f.path,
        NEW.content
    FROM files f
    WHERE f.id = NEW.file_id;
END;

-- Trigger: Remove indexed chunk from FTS5 on deletion
CREATE TRIGGER IF NOT EXISTS trig_chunks_after_delete AFTER DELETE ON content_chunks
BEGIN
    DELETE FROM fts_content WHERE chunk_id = OLD.id;
END;

-- Trigger: Re-index chunk in FTS5 on content update
CREATE TRIGGER IF NOT EXISTS trig_chunks_after_update AFTER UPDATE ON content_chunks
BEGIN
    DELETE FROM fts_content WHERE chunk_id = OLD.id;
    INSERT INTO fts_content (chunk_id, file_id, filename, path, content)
    SELECT
        NEW.id,
        NEW.file_id,
        f.filename,
        f.path,
        NEW.content
    FROM files f
    WHERE f.id = NEW.file_id;
END;

-- Trigger: Update paths and filenames in FTS5 when a file is moved or renamed
CREATE TRIGGER IF NOT EXISTS trig_files_after_rename AFTER UPDATE OF path, filename ON files
BEGIN
    UPDATE fts_content
    SET path = NEW.path, filename = NEW.filename
    WHERE file_id = NEW.id;
END;
