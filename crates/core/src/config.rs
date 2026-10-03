use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Global configuration for the NaviFS daemon and subsystems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub engine_name: String,
    pub data_dir: PathBuf,
    pub watch_paths: Vec<WatchDirectoryConfig>,
    pub database: DatabaseConfig,
    pub mcp: McpConfig,
    pub indexing: IndexingConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        let default_data_dir = PathBuf::from(home).join(".navifs");

        Self {
            engine_name: "NaviFS Intelligent Engine".to_string(),
            database: DatabaseConfig {
                db_path: default_data_dir.join("navifs.db"),
                max_connections: 5,
            },
            data_dir: default_data_dir,
            watch_paths: Vec::new(),
            mcp: McpConfig::default(),
            indexing: IndexingConfig::default(),
        }
    }
}

/// Directory path configured for background file indexing and watching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchDirectoryConfig {
    pub path: PathBuf,
    pub recursive: bool,
    pub ignore_patterns: Vec<String>,
}

impl WatchDirectoryConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            recursive: true,
            ignore_patterns: vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/.venv/**".to_string(),
                "**/.DS_Store".to_string(),
                "**/Thumbs.db".to_string(),
            ],
        }
    }
}

/// Local SQLite storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub db_path: PathBuf,
    pub max_connections: u32,
}

/// Model Context Protocol (MCP) server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub server_name: String,
    pub server_version: String,
    pub transport: McpTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    Stdio,
    Sse { port: u16 },
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server_name: "navifs-mcp".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            transport: McpTransport::Stdio,
        }
    }
}

/// Indexing pipeline and chunking settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    pub max_file_size_bytes: u64,
    pub chunk_size_chars: usize,
    pub chunk_overlap_chars: usize,
    pub debounce_delay_ms: u64,
    pub worker_threads: usize,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            max_file_size_bytes: 50 * 1024 * 1024, // 50 MB
            chunk_size_chars: 1500,
            chunk_overlap_chars: 200,
            debounce_delay_ms: 500,
            worker_threads: 4,
        }
    }
}
