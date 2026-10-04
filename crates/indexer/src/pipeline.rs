//! Indexing pipeline orchestrating scanner, watcher, extraction, and database persistence

use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, info, warn};
use navifs_core::{
    ContentHash, DatabaseStore, FileIdentity, FileTemporalEvent, NaviError, Result, SearchProvider,
};
use navifs_extract::ExtractorRegistry;
use crate::watcher::FsChangeEvent;

/// Main pipeline coordinator that runs extraction and writes to database and search indices
pub struct IndexingPipeline {
    db: Arc<dyn DatabaseStore>,
    search: Arc<dyn SearchProvider>,
    registry: Arc<ExtractorRegistry>,
}

impl IndexingPipeline {
    pub fn new(
        db: Arc<dyn DatabaseStore>,
        search: Arc<dyn SearchProvider>,
        registry: Arc<ExtractorRegistry>,
    ) -> Self {
        Self {
            db,
            search,
            registry,
        }
    }

    /// Process a single file: read bytes, compute SHA-256, extract chunks/entities, and save
    pub async fn process_file(&self, path: &Path) -> Result<FileIdentity> {
        let metadata = tokio::fs::metadata(path).await.map_err(NaviError::Io)?;
        let modified = metadata
            .modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t))
            .unwrap_or_else(|_| chrono::Utc::now());

        let mut identity = FileIdentity::new(path, metadata.len(), modified);

        // Read file bytes and compute SHA-256 content hash
        let file_bytes = tokio::fs::read(path).await.map_err(NaviError::Io)?;
        let current_hash = ContentHash::from_bytes(&file_bytes);
        identity = identity.with_hash(current_hash.clone());

        // Check if existing file has identical SHA-256 hash
        let is_new = match self.db.get_file_by_path(identity.fingerprint.as_str()).await? {
            Some(existing) => {
                if let Some(ref prev_hash) = existing.content_hash {
                    if prev_hash == &current_hash && existing.size_bytes == metadata.len() {
                        debug!("File SHA-256 matches database record, skipping: {:?}", path);
                        return Ok(existing);
                    }
                }
                identity.id = existing.id;
                false
            }
            None => true,
        };

        // Record creation or modification lifecycle event (§13 & §17.1)
        if is_new {
            let evt = FileTemporalEvent::new(
                identity.id,
                "created",
                "scanner",
                Some(serde_json::json!({
                    "size_bytes": identity.size_bytes,
                    "mime_type": identity.mime_type.as_str(),
                })),
            );
            let _ = self.db.record_event(&evt).await;
        } else {
            let evt = FileTemporalEvent::new(
                identity.id,
                "modified",
                "watcher",
                Some(serde_json::json!({
                    "size_bytes": identity.size_bytes,
                    "mime_type": identity.mime_type.as_str(),
                })),
            );
            let _ = self.db.record_event(&evt).await;
        }

        // Upsert preliminary file record
        self.db.upsert_file(&identity).await?;

        // Run multi-modal extraction (plain text, markdown, json, pdf, xlsx, docx)
        let extraction = self.registry.extract_file(&identity, path).await?;
        info!(
            "Extracted {:?} ({} chunks, {} entities, {} relations)",
            identity.fingerprint.as_str(),
            extraction.chunks.len(),
            extraction.entities.len(),
            extraction.relations.len()
        );

        // Persist chunks, entities, and relations
        if !extraction.chunks.is_empty() {
            // Delete old chunks if updating
            self.db.delete_chunks_for_file(&identity.id).await?;
            self.db.save_chunks(&extraction.chunks).await?;
            self.search.index_chunks(&extraction.chunks).await?;
        }

        if !extraction.entities.is_empty() {
            self.db.save_entities(&extraction.entities).await?;
        }

        if !extraction.relations.is_empty() {
            self.db.save_relations(&extraction.relations).await?;
        }

        identity.mark_indexed();
        self.db.upsert_file(&identity).await?;

        // Record indexed event
        let index_evt = FileTemporalEvent::new(
            identity.id,
            "indexed",
            "pipeline",
            Some(serde_json::json!({
                "chunks_count": extraction.chunks.len(),
                "entities_count": extraction.entities.len(),
            })),
        );
        let _ = self.db.record_event(&index_evt).await;

        Ok(identity)
    }

    /// Handles file deletion
    pub async fn handle_deletion(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        if let Some(existing) = self.db.get_file_by_path(&path_str).await? {
            let del_evt = FileTemporalEvent::new(
                existing.id,
                "deleted",
                "watcher",
                Some(serde_json::json!({ "path": path_str.to_string() })),
            );
            let _ = self.db.record_event(&del_evt).await;

            self.search.remove_file_from_index(&existing.id).await?;
            self.db.delete_chunks_for_file(&existing.id).await?;
            self.db.delete_file(&existing.id).await?;
            info!("Successfully purged deleted file from index: {}", path_str);
        }
        Ok(())
    }

    /// Consumes change events from the NotifyWatcher asynchronously
    pub async fn run_watcher_loop(&self, mut rx: Receiver<FsChangeEvent>) {
        info!("Starting live watcher processing loop...");

        while let Some(event) = rx.recv().await {
            match event {
                FsChangeEvent::Created { path, hash } => {
                    info!("Watcher detected creation: {:?} (SHA-256: {})", path, hash);
                    if let Err(e) = self.process_file(&path).await {
                        warn!("Failed processing created file {:?}: {}", path, e);
                    }
                }
                FsChangeEvent::Modified { path, new_hash } => {
                    info!("Watcher detected modification: {:?} (SHA-256: {})", path, new_hash);
                    if let Err(e) = self.process_file(&path).await {
                        warn!("Failed processing modified file {:?}: {}", path, e);
                    }
                }
                FsChangeEvent::Deleted { path } => {
                    info!("Watcher detected deletion: {:?}", path);
                    if let Err(e) = self.handle_deletion(&path).await {
                        warn!("Failed processing deleted file {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Watcher event processing loop finished.");
    }
}
