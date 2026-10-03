//! Filesystem scanning, live watching, and indexing pipeline coordination

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;
use navifs_core::{
    ChangeKind, DatabaseStore, ExtractionOutput, FileIdentity, FsEvent, NaviError, Result,
    SearchProvider, WatchDirectoryConfig,
};
use navifs_extract::ExtractorRegistry;

/// Pipeline coordinator that extracts and persists files into the database and search index
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

    /// Process a single file through the entire pipeline: extraction, storage, and indexing
    pub async fn process_file(&self, path: &Path) -> Result<FileIdentity> {
        let metadata = tokio::fs::metadata(path).await.map_err(NaviError::Io)?;
        let modified = metadata
            .modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t))
            .unwrap_or_else(|_| chrono::Utc::now());

        let mut identity = FileIdentity::new(path, metadata.len(), modified);

        // Check if existing file needs update
        if let Some(existing) = self.db.get_file_by_path(identity.fingerprint.as_str()).await? {
            if existing.modified_at == modified && existing.size_bytes == metadata.len() {
                debug!("File unchanged, skipping: {}", identity.fingerprint.as_str());
                return Ok(existing);
            }
            identity.id = existing.id;
        }

        // Compute hash
        if let Ok(file_bytes) = tokio::fs::read(path).await {
            let hash = navifs_core::ContentHash::from_bytes(&file_bytes);
            identity = identity.with_hash(hash);
        }

        // Upsert file metadata
        self.db.upsert_file(&identity).await?;

        // Extract content
        let extraction = self.registry.extract_file(&identity, path).await?;
        info!(
            "Extracted {} ({} chunks, {} entities)",
            identity.fingerprint.as_str(),
            extraction.chunks.len(),
            extraction.entities.len()
        );

        // Save chunks and entities to database
        if !extraction.chunks.is_empty() {
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

        Ok(identity)
    }

    /// Handles deletion of a tracked file
    pub async fn handle_deletion(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        if let Some(existing) = self.db.get_file_by_path(&path_str).await? {
            self.search.remove_file_from_index(&existing.id).await?;
            self.db.delete_chunks_for_file(&existing.id).await?;
            self.db.delete_file(&existing.id).await?;
            info!("Successfully purged deleted file: {}", path_str);
        }
        Ok(())
    }
}

/// Directory scanner that recursively walks paths and identifies targets
pub struct DirectoryScanner;

impl DirectoryScanner {
    pub fn scan(config: &WatchDirectoryConfig) -> Vec<PathBuf> {
        let mut results = Vec::new();
        if !config.path.exists() {
            warn!("Configured path does not exist: {:?}", config.path);
            return results;
        }

        let walker = WalkDir::new(&config.path).follow_links(false);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let p = entry.path();
                let p_str = p.to_string_lossy();

                let should_ignore = config.ignore_patterns.iter().any(|pattern| {
                    let clean = pattern.trim_matches('*').trim_matches('/');
                    p_str.contains(clean)
                });

                if !should_ignore {
                    results.push(p.to_path_buf());
                }
            }
        }

        results
    }
}
