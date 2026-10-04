//! Filesystem scanning, live notify watching, and indexing pipeline coordination

pub mod exclusion;
pub mod pipeline;
pub mod scanner;
pub mod watcher;

pub use exclusion::PathExclusionFilter;
pub use pipeline::IndexingPipeline;
pub use scanner::{RecursiveScanner, ScanReport};
pub use watcher::{FsChangeEvent, NotifyWatcher};
