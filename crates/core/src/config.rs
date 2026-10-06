use crate::error::{NaviError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Environment variable that overrides the NaviFS data directory
pub const DATA_DIR_ENV: &str = "NAVIFS_DATA_DIR";

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

impl EngineConfig {
    /// Builds the configuration, resolving the data directory in this order:
    /// explicit override, the `NAVIFS_DATA_DIR` environment variable, then the platform
    /// data directory (`%LOCALAPPDATA%\navifs`, `~/.local/share/navifs`,
    /// `~/Library/Application Support/navifs`).
    pub fn discover(data_dir_override: Option<PathBuf>) -> Result<Self> {
        let data_dir = match data_dir_override {
            Some(dir) => dir,
            None => match std::env::var_os(DATA_DIR_ENV) {
                Some(dir) if !dir.is_empty() => PathBuf::from(dir),
                _ => directories::ProjectDirs::from("dev", "navifs", "navifs")
                    .map(|dirs| dirs.data_dir().to_path_buf())
                    .ok_or_else(|| {
                        NaviError::ConfigError(format!(
                            "could not determine a user data directory; set {DATA_DIR_ENV}"
                        ))
                    })?,
            },
        };

        Ok(Self {
            engine_name: "NaviFS".to_string(),
            database: DatabaseConfig {
                db_path: data_dir.join("navifs.db"),
                max_connections: 5,
            },
            data_dir,
            watch_paths: Vec::new(),
            mcp: McpConfig::default(),
            indexing: IndexingConfig::default(),
        })
    }

    /// Directory where downloaded embedding models are cached
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
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
