//! Recursive filesystem scanner respecting OS permissions and enforcing path exclusions

use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;
use navifs_core::{ContentHash, DatabaseStore, Result, WatchDirectoryConfig};
use crate::exclusion::PathExclusionFilter;

/// Result summary of a recursive scan
#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    pub scanned_files: Vec<PathBuf>,
    pub modified_or_new: Vec<PathBuf>,
    pub unchanged_count: usize,
    pub permission_denied_paths: Vec<PathBuf>,
}

/// Recursive scanner traversing filesystem trees and discovering files
pub struct RecursiveScanner {
    filter: PathExclusionFilter,
}

impl RecursiveScanner {
    pub fn new(filter: PathExclusionFilter) -> Self {
        Self { filter }
    }

    pub fn from_config(config: &WatchDirectoryConfig) -> Self {
        let filter = PathExclusionFilter::new(config.ignore_patterns.clone(), true);
        Self { filter }
    }

    /// Recursively discovers all accessible non-excluded files in target path
    pub fn scan_path(&self, root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut accessible_files = Vec::new();
        let mut permission_denied = Vec::new();

        if !root.exists() {
            warn!("Root path does not exist: {:?}", root);
            return (accessible_files, permission_denied);
        }

        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                // Prune excluded directories immediately to avoid traversing massive subtrees
                let path = entry.path();
                let excluded = self.filter.should_exclude(path);
                if excluded {
                    debug!("Pruning excluded directory/file from scan: {:?}", path);
                }
                !excluded
            });

        for item in walker {
            match item {
                Ok(entry) => {
                    // Only process regular files
                    if entry.file_type().is_file() {
                        let path = entry.path().to_path_buf();
                        accessible_files.push(path);
                    }
                }
                Err(err) => {
                    if let Some(path) = err.path() {
                        warn!("Permission denied or inaccessible path during scan: {:?} ({})", path, err);
                        permission_denied.push(path.to_path_buf());
                    } else {
                        warn!("WalkDir encountered system I/O error: {}", err);
                    }
                }
            }
        }

        info!(
            "Recursive scan of {:?} completed: found {} accessible files, {} skipped due to permissions",
            root,
            accessible_files.len(),
            permission_denied.len()
        );

        (accessible_files, permission_denied)
    }

    /// Scans root path and performs SHA-256 change detection against stored database state
    pub async fn scan_and_detect_changes(
        &self,
        root: &Path,
        db: &dyn DatabaseStore,
    ) -> Result<ScanReport> {
        let (files, permission_denied) = self.scan_path(root);
        let mut report = ScanReport {
            scanned_files: files.clone(),
            modified_or_new: Vec::new(),
            unchanged_count: 0,
            permission_denied_paths: permission_denied,
        };

        for path in files {
            let path_str = path.to_string_lossy();
            let existing_record = match db.get_file_by_path(&path_str).await {
                Ok(rec) => rec,
                Err(e) => {
                    warn!("Database lookup error for {:?}: {}", path, e);
                    None
                }
            };

            match tokio::fs::read(&path).await {
                Ok(bytes) => {
                    let current_hash = ContentHash::from_bytes(&bytes);

                    match existing_record {
                        Some(ref existing) => {
                            if let Some(ref prev_hash) = existing.content_hash {
                                if prev_hash == &current_hash {
                                    report.unchanged_count += 1;
                                    continue;
                                }
                            }
                            // Hash changed or was not set
                            debug!("SHA-256 changed for {:?}", path);
                            report.modified_or_new.push(path);
                        }
                        None => {
                            // Brand new file
                            debug!("New file discovered: {:?}", path);
                            report.modified_or_new.push(path);
                        }
                    }
                }
                Err(err) => {
                    warn!("Unable to read file for SHA-256 detection {:?}: {}", path, err);
                    report.permission_denied_paths.push(path);
                }
            }
        }

        info!(
            "SHA-256 change detection report: {} new/modified, {} unchanged, {} inaccessible",
            report.modified_or_new.len(),
            report.unchanged_count,
            report.permission_denied_paths.len()
        );

        Ok(report)
    }
}
