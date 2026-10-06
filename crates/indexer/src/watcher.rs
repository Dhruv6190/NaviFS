//! Real-time filesystem watcher using notify with SHA-256 change detection and path exclusions

use crate::exclusion::PathExclusionFilter;
use navifs_core::{ContentHash, NaviError, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{debug, info, trace, warn};

/// High-level change event verified by SHA-256 hash comparison
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsChangeEvent {
    Created {
        path: PathBuf,
        hash: ContentHash,
    },
    Modified {
        path: PathBuf,
        new_hash: ContentHash,
    },
    Deleted {
        path: PathBuf,
    },
}

/// Notify-based filesystem watcher that filters exclusions and deduplicates via SHA-256
pub struct NotifyWatcher {
    #[allow(dead_code)]
    filter: PathExclusionFilter,
    watcher: RecommendedWatcher,
    #[allow(dead_code)]
    watched_paths: Vec<PathBuf>,
    #[allow(dead_code)]
    recent_hashes: Arc<Mutex<HashMap<PathBuf, (ContentHash, Instant)>>>,
    #[allow(dead_code)]
    debounce_duration: Duration,
}

impl NotifyWatcher {
    /// Spawns a notify watcher and returns an async receiver for verified change events
    pub fn new(
        filter: PathExclusionFilter,
        debounce_ms: u64,
    ) -> Result<(Self, Receiver<FsChangeEvent>)> {
        let (tx, rx) = mpsc::channel(256);
        let recent_hashes: Arc<Mutex<HashMap<PathBuf, (ContentHash, Instant)>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let hashes_clone = recent_hashes.clone();
        let filter_clone = filter.clone();
        let debounce = Duration::from_millis(debounce_ms.max(100));

        let event_handler = move |res: notify::Result<Event>| match res {
            Ok(event) => {
                trace!("Raw notify event: {:?}", event);
                Self::handle_raw_event(event, &filter_clone, &hashes_clone, debounce, &tx);
            }
            Err(e) => {
                warn!("Filesystem watcher error: {}", e);
            }
        };

        let watcher = RecommendedWatcher::new(event_handler, Config::default()).map_err(|e| {
            NaviError::IndexingError(format!("Failed to initialize notify watcher: {}", e))
        })?;

        let instance = Self {
            filter,
            watcher,
            watched_paths: Vec::new(),
            recent_hashes,
            debounce_duration: debounce,
        };

        Ok((instance, rx))
    }

    /// Watches a target directory recursively
    pub fn watch_path(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(NaviError::FileNotFound(path.to_path_buf()));
        }

        self.watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| NaviError::IndexingError(format!("Failed to watch {:?}: {}", path, e)))?;

        info!("Watcher successfully attached to: {:?}", path);
        self.watched_paths.push(path.to_path_buf());
        Ok(())
    }

    /// Stops watching a target directory
    pub fn unwatch_path(&mut self, path: &Path) -> Result<()> {
        self.watcher.unwatch(path).map_err(|e| {
            NaviError::IndexingError(format!("Failed to unwatch {:?}: {}", path, e))
        })?;

        self.watched_paths.retain(|p| p != path);
        Ok(())
    }

    /// Handles a raw notify event, checks exclusions, and compares SHA-256 hashes
    fn handle_raw_event(
        event: Event,
        filter: &PathExclusionFilter,
        hashes: &Arc<Mutex<HashMap<PathBuf, (ContentHash, Instant)>>>,
        debounce: Duration,
        tx: &Sender<FsChangeEvent>,
    ) {
        for path in event.paths {
            // 1. Enforce path exclusions
            if filter.should_exclude(&path) {
                trace!("Watcher ignoring excluded path: {:?}", path);
                continue;
            }

            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {
                    // Check if path is a file and accessible
                    if path.is_file() {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                let new_hash = ContentHash::from_bytes(&bytes);
                                let now = Instant::now();

                                let mut cache = match hashes.lock() {
                                    Ok(c) => c,
                                    Err(_) => return,
                                };

                                if let Some((prev_hash, last_time)) = cache.get(&path) {
                                    // Debounce and hash equality check
                                    if prev_hash == &new_hash
                                        && now.duration_since(*last_time) < debounce
                                    {
                                        trace!("Deduplicating identical SHA-256 for: {:?}", path);
                                        continue;
                                    }

                                    if prev_hash == &new_hash {
                                        trace!("SHA-256 unchanged on modify for: {:?}", path);
                                        continue;
                                    }

                                    cache.insert(path.clone(), (new_hash.clone(), now));
                                    let change = FsChangeEvent::Modified {
                                        path: path.clone(),
                                        new_hash,
                                    };
                                    let _ = tx.blocking_send(change);
                                } else {
                                    // Brand new creation
                                    cache.insert(path.clone(), (new_hash.clone(), now));
                                    let change = FsChangeEvent::Created {
                                        path: path.clone(),
                                        hash: new_hash,
                                    };
                                    let _ = tx.blocking_send(change);
                                }
                            }
                            Err(e) => {
                                trace!(
                                    "Could not read file during watcher event {:?}: {}",
                                    path,
                                    e
                                );
                            }
                        }
                    }
                }
                EventKind::Remove(_) => {
                    if let Ok(mut cache) = hashes.lock() {
                        cache.remove(&path);
                    }
                    debug!("Watcher detected deletion: {:?}", path);
                    let _ = tx.blocking_send(FsChangeEvent::Deleted { path });
                }
                _ => {}
            }
        }
    }
}
