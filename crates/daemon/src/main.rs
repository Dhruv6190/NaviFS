//! NaviFS Daemon and CLI
//!
//! Entry point for indexing local file systems and exposing them via Model Context Protocol (MCP).

use std::path::PathBuf;
use std::sync::Arc;
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use navifs_core::{DatabaseStore, EngineConfig, SearchProvider, WatchDirectoryConfig};
use navifs_database::SqliteDatabase;
use navifs_extract::ExtractorRegistry;
use navifs_graph::KnowledgeGraph;
use navifs_indexer::{DirectoryScanner, IndexingPipeline};
use navifs_mcp::McpServer;
use navifs_search::LexicalSearchEngine;

#[derive(Parser, Debug)]
#[command(name = "navifs")]
#[command(author = "NaviFS Team")]
#[command(version = "0.1.0")]
#[command(about = "Local-first filesystem intelligent engine exposed via Model Context Protocol (MCP)", long_about = None)]
struct Cli {
    #[arg(short, long, help = "Path to custom SQLite database file")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Index a folder or workspace recursively
    Index {
        #[arg(help = "Directory path to index")]
        path: PathBuf,
    },

    /// Search indexed files using lexical and semantic matching
    Search {
        #[arg(help = "Search query keywords")]
        query: String,

        #[arg(short, long, default_value_t = 10, help = "Max results to return")]
        limit: usize,
    },

    /// Run the Model Context Protocol (MCP) server over standard I/O (stdio)
    Mcp,

    /// Run as a background daemon with live filesystem watching
    Daemon {
        #[arg(short, long, help = "Directory to watch and index")]
        watch: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Check if we are running MCP mode - if so, suppress stdout logs to avoid corrupting JSON-RPC
    let is_mcp = matches!(cli.command, Commands::Mcp);
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if is_mcp { Level::ERROR } else { Level::INFO })
        .with_writer(if is_mcp { std::io::stderr } else { std::io::stdout as _ })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config = EngineConfig::default();
    let db_path = cli.db.unwrap_or(config.database.db_path);

    info!("Initializing NaviFS SQLite database at {:?}", db_path);
    let db = Arc::new(SqliteDatabase::new(db_path)?);
    db.initialize().await?;

    let search = Arc::new(LexicalSearchEngine::new());
    let graph = Arc::new(KnowledgeGraph::new());
    let registry = Arc::new(ExtractorRegistry::new());

    let pipeline = Arc::new(IndexingPipeline::new(
        db.clone(),
        search.clone(),
        registry.clone(),
    ));

    match cli.command {
        Commands::Index { path } => {
            println!("🚀 NaviFS: Indexing directory {:?}", path);
            let watch_config = WatchDirectoryConfig::new(&path);
            let files = DirectoryScanner::scan(&watch_config);
            println!("Found {} files to process...", files.len());

            for (i, file_path) in files.iter().enumerate() {
                match pipeline.process_file(file_path).await {
                    Ok(identity) => {
                        println!(
                            "[{}/{}] Indexed: {} ({})",
                            i + 1,
                            files.len(),
                            identity.fingerprint.filename(),
                            identity.mime_type
                        );
                    }
                    Err(e) => {
                        eprintln!("[{}/{}] Failed {}: {}", i + 1, files.len(), file_path.display(), e);
                    }
                }
            }
            println!("✅ Indexing complete!");
        }

        Commands::Search { query, limit } => {
            println!("🔍 Searching for: \"{}\"", query);
            let hits = search.search(&query, limit).await?;
            if hits.is_empty() {
                println!("No matching chunks found.");
            } else {
                for (i, hit) in hits.iter().enumerate() {
                    println!("\n--- Hit {} (Score: {:.2}) ---", i + 1, hit.score);
                    println!("{}", hit.content.trim());
                }
            }
        }

        Commands::Mcp => {
            let mcp_server = McpServer::new(db, search, graph);
            mcp_server.run_stdio().await?;
        }

        Commands::Daemon { watch } => {
            println!("🌐 NaviFS Daemon active.");
            if let Some(path) = watch {
                println!("Watching directory: {:?}", path);
                let watch_config = WatchDirectoryConfig::new(&path);
                let files = DirectoryScanner::scan(&watch_config);
                for file_path in files {
                    let _ = pipeline.process_file(&file_path).await;
                }
            }

            println!("Starting integrated MCP server on stdio...");
            let mcp_server = McpServer::new(db, search, graph);
            mcp_server.run_stdio().await?;
        }
    }

    Ok(())
}
