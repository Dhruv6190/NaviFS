//! NaviFS Daemon and CLI
//!
//! Entry point for indexing local file systems and exposing them via Model Context Protocol (MCP).

pub mod setup;

use std::path::PathBuf;
use std::sync::Arc;
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use navifs_core::{DatabaseStore, EngineConfig, WatchDirectoryConfig};
use navifs_database::SqliteDatabase;
use navifs_extract::ExtractorRegistry;
use navifs_graph::KnowledgeGraph;
use navifs_indexer::IndexingPipeline;
use navifs_mcp::McpServer;
use navifs_search::{HybridSearchEngine, HybridSearchQuery, LexicalSearchEngine};

#[derive(Parser, Debug)]
#[command(name = "navifs")]
#[command(author = "NaviFS Team")]
#[command(version = "0.1.0")]
#[command(about = "Local-first filesystem intelligent engine exposed via Model Context Protocol (MCP)", long_about = None)]
struct Cli {
    #[arg(short, long, help = "Path to custom SQLite database file")]
    db: Option<PathBuf>,

    #[arg(long, help = "Run the MCP server over standard I/O (stdio) directly")]
    stdio: bool,

    #[command(subcommand)]
    command: Option<Commands>,
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
    #[command(alias = "stdio")]
    Mcp {
        #[arg(short, long, help = "Optional workspace path to index before launching MCP server")]
        path: Option<PathBuf>,
    },

    /// Run as a background daemon with live filesystem watching
    Daemon {
        #[arg(short, long, help = "Directory to watch and index")]
        watch: Option<PathBuf>,
    },

    /// Automatically configure NaviFS for installed AI agents (Claude, Cursor, Antigravity, Windsurf, VS Code)
    Setup {
        /// Automatically configure all detected AI agents without prompting
        #[arg(short, long)]
        all: bool,

        /// Target client to configure: claude, cursor, antigravity, windsurf, vscode
        #[arg(short, long)]
        client: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Default to Mcp stdio mode if no subcommand is passed or if --stdio flag is set
    let command = match (cli.stdio, cli.command) {
        (true, _) => Commands::Mcp { path: None },
        (false, Some(cmd)) => cmd,
        (false, None) => Commands::Mcp { path: None },
    };

    if let Commands::Setup { all, client } = command {
        return setup::run_setup(all, client);
    }

    // Check if we are running MCP mode - if so, suppress stdout logs to avoid corrupting JSON-RPC
    let is_mcp = matches!(command, Commands::Mcp { .. });
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if is_mcp { Level::ERROR } else { Level::INFO })
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config = EngineConfig::default();
    let db_path = cli.db.unwrap_or(config.database.db_path);

    info!("Initializing NaviFS SQLite database at {:?}", db_path);
    let db = Arc::new(SqliteDatabase::new(db_path)?);
    db.initialize().await?;

    let search = Arc::new(LexicalSearchEngine::new());
    let hybrid_search = Arc::new(HybridSearchEngine::new(db.clone()));
    let graph = Arc::new(KnowledgeGraph::new());
    let registry = Arc::new(ExtractorRegistry::new());

    let pipeline = Arc::new(IndexingPipeline::new(
        db.clone(),
        search.clone(),
        registry.clone(),
    ));

    match command {
        Commands::Index { path } => {
            println!("🚀 NaviFS: Recursively indexing directory {:?}", path);
            let watch_config = WatchDirectoryConfig::new(&path);
            let scanner = navifs_indexer::RecursiveScanner::from_config(&watch_config);
            let (files, permission_denied) = scanner.scan_path(&path);
            println!("Discovered {} files ({} skipped due to permissions/exclusion)...", files.len(), permission_denied.len());

            for (i, file_path) in files.iter().enumerate() {
                match pipeline.process_file(file_path).await {
                    Ok(identity) => {
                        println!(
                            "[{}/{}] Indexed: {} ({}) [SHA-256: {}]",
                            i + 1,
                            files.len(),
                            identity.fingerprint.filename(),
                            identity.mime_type,
                            identity.content_hash.as_ref().map(|h| &h.as_str()[..8]).unwrap_or("none")
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
            println!("🔍 NaviFS Hybrid Search for: \"{}\"", query);
            let hybrid_query = HybridSearchQuery::new(&query).with_limit(limit);
            let candidates = hybrid_search.search_hybrid(hybrid_query).await?;
            if candidates.is_empty() {
                println!("No matching candidates found.");
            } else {
                for (i, c) in candidates.iter().enumerate() {
                    println!("\n========================================================");
                    println!("🏆 Candidate #{} | Score: {:.3} | {}", i + 1, c.score, c.evidence.locator_summary);
                    println!("📁 Path: {}", c.path);
                    println!("📊 Features: Lexical: {:.2} | Path: {:.2} | Freshness: {:.2} | Quality: {:.2}",
                        c.features.lexical_score,
                        c.features.path_similarity,
                        c.features.freshness_score,
                        c.features.extraction_quality
                    );
                    if let Some(ref lines) = c.evidence.line_range {
                        println!("📍 Line Bounds: {}-{}", lines.start_line, lines.end_line);
                    }
                    if let Some(ref pages) = c.evidence.page_range {
                        println!("📄 Page Bounds: {}-{}", pages.start_page, pages.end_page);
                    }
                    println!("---------------- Evidence Snippet ----------------");
                    let snippet = if !c.evidence.snippet.is_empty() {
                        &c.evidence.snippet
                    } else {
                        &c.evidence.content
                    };
                    println!("{}", snippet.trim());
                }
            }
        }

        Commands::Mcp { path } => {
            if let Some(ref p) = path {
                let watch_config = WatchDirectoryConfig::new(p);
                let scanner = navifs_indexer::RecursiveScanner::from_config(&watch_config);
                let (files, _) = scanner.scan_path(p);
                for file_path in files {
                    let _ = pipeline.process_file(&file_path).await;
                }
            }
            let mcp_server = McpServer::new(db, search, graph)
                .with_hybrid_engine(hybrid_search);
            mcp_server.run_stdio().await?;
        }

        Commands::Daemon { watch } => {
            println!("🌐 NaviFS Daemon active.");
            if let Some(path) = watch {
                println!("Scanning and attaching live watcher to directory: {:?}", path);
                let watch_config = WatchDirectoryConfig::new(&path);
                let scanner = navifs_indexer::RecursiveScanner::from_config(&watch_config);
                let (files, _) = scanner.scan_path(&path);
                for file_path in files {
                    let _ = pipeline.process_file(&file_path).await;
                }

                // Initialize real-time notify watcher
                let filter = navifs_indexer::PathExclusionFilter::new(watch_config.ignore_patterns, true);
                if let Ok((mut watcher, rx)) = navifs_indexer::NotifyWatcher::new(filter, 300) {
                    if let Ok(()) = watcher.watch_path(&path) {
                        let pipeline_clone = pipeline.clone();
                        tokio::spawn(async move {
                            pipeline_clone.run_watcher_loop(rx).await;
                        });
                        println!("⚡ Real-time file system watcher active with SHA-256 change detection.");
                    }
                }
            }

            println!("Starting integrated MCP server on stdio...");
            let mcp_server = McpServer::new(db, search, graph)
                .with_hybrid_engine(hybrid_search);
            mcp_server.run_stdio().await?;
        }

        Commands::Setup { .. } => unreachable!(),
    }

    Ok(())
}
