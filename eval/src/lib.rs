//! Automated integration testing and evaluation harness for NaviFS MCP tools.
//!
//! Validates:
//! 1. Sample directory seeding (code, markdown, json, text).
//! 2. Indexing pipeline execution (SHA-256 detection, SQLite persistence, FTS5 triggers).
//! 3. Complete tool trace execution:
//!    - `initialize` (verifying embedded instructions in headers)
//!    - `tools/list` (verifying schema conformance)
//!    - `search` (queries, rank fusion, feature vector reranking, evidence)
//!    - `inspect` (metadata summaries, hashes, outline entities)
//!    - `related` (one-to-one hop graph relationship traversal)
//!    - `open` (bounded content range retrieval with line/byte bounds)
//! 4. Full output specification verification.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use navifs_core::{
    DatabaseStore, EntityNode, EntityType, RelationEdge, RelationType, WatchDirectoryConfig,
};
use navifs_database::SqliteDatabase;
use navifs_extract::ExtractorRegistry;
use navifs_graph::KnowledgeGraph;
use navifs_indexer::{IndexingPipeline, RecursiveScanner};
use navifs_mcp::{
    BoundedContentResponse, JsonRpcRequest, McpServer, MetadataSummary, OneHopGraphResponse,
};
use navifs_search::{CandidateResult, HybridSearchEngine, LexicalSearchEngine};

/// Summary report of an automated evaluation run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub sample_dir: String,
    pub files_seeded: usize,
    pub chunks_created: usize,
    pub protocol_version: String,
    pub server_instructions_verified: bool,
    pub tools_registered: Vec<String>,
    pub search_verified: bool,
    pub top_search_candidate: String,
    pub top_search_score: f32,
    pub inspect_verified: bool,
    pub inspected_file_hash: String,
    pub inspected_chunk_count: usize,
    pub outline_entity_count: usize,
    pub related_verified: bool,
    pub graph_connections: usize,
    pub open_verified: bool,
    pub opened_locator: String,
    pub opened_lines_retrieved: usize,
    pub all_specs_passed: bool,
}

/// Seeds a realistic sample directory for evaluation
pub fn seed_sample_directory(base_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fs::create_dir_all(base_dir)?;

    let mut seeded = Vec::new();

    // 1. Rust Service Code
    let auth_rs_path = base_dir.join("auth_service.rs");
    let auth_rs_content = r#"//! Authentication Service Implementation
//! Handles user verification and token generation.

use std::collections::HashMap;

/// Core authentication service managing active user sessions.
pub struct AuthService {
    sessions: HashMap<String, String>,
    token_secret: String,
}

impl AuthService {
    /// Initialize a new AuthService with a signing secret.
    pub fn new(token_secret: impl Into<String>) -> Self {
        Self {
            sessions: HashMap::new(),
            token_secret: token_secret.into(),
        }
    }

    /// Verify an incoming JSON Web Token (JWT) signature and expiration.
    pub fn verify_jwt_token(&self, token: &str) -> Result<String, String> {
        if token.is_empty() {
            return Err("Token is empty".to_string());
        }
        if token.starts_with("bearer_") {
            Ok("user_admin".to_string())
        } else {
            Err("Invalid token signature".to_string())
        }
    }

    /// Hash password securely using SHA-256 cryptographic hashing.
    pub fn hash_password(&self, raw: &str) -> String {
        format!("hash_{}", raw)
    }
}
"#;
    fs::write(&auth_rs_path, auth_rs_content)?;
    seeded.push(auth_rs_path);

    // 2. Architecture Markdown Document
    let arch_md_path = base_dir.join("architecture.md");
    let arch_md_content = r#"# System Architecture

Welcome to the NaviFS high-performance distributed architecture.

## Security Layer
The security subsystem delegates authentication to `auth_service.rs`.
All API requests pass through the authentication filter before hitting storage.

### JWT Verification
The `verify_jwt_token` function validates bearer tokens against session records.
Revoked tokens are rejected immediately.

## Storage Subsystem
Data persistence is backed by local-first SQLite databases with FTS5 lexical indexing.
All file chunks and relation edges are stored with ACID guarantees.
"#;
    fs::write(&arch_md_path, arch_md_content)?;
    seeded.push(arch_md_path);

    // 3. JSON Configuration
    let config_json_path = base_dir.join("config.json");
    let config_json_content = r#"{
  "service_name": "AuthService",
  "auth_endpoint": "/api/v1/auth",
  "token_expiry_seconds": 3600,
  "allowed_algorithms": ["HS256", "RS256"]
}
"#;
    fs::write(&config_json_path, config_json_content)?;
    seeded.push(config_json_path);

    // 4. Plain Text User Guide
    let guide_txt_path = base_dir.join("user_guide.txt");
    let guide_txt_content = r#"NaviFS User Guide
=================
To configure authentication, review architecture.md and instantiate AuthService.
Token verification is performed via verify_jwt_token.
Ensure all secret keys are managed securely.
"#;
    fs::write(&guide_txt_path, guide_txt_content)?;
    seeded.push(guide_txt_path);

    Ok(seeded)
}

/// Executes the complete evaluation workflow and verifies all outputs against the spec
pub async fn run_evaluation() -> anyhow::Result<EvalReport> {
    let eval_id = uuid::Uuid::new_v4();
    let temp_root = std::env::temp_dir().join(format!("navifs_eval_{}", eval_id));
    let sample_dir = temp_root.join("workspace");
    let db_path = temp_root.join("eval.db");

    println!("================================================================================");
    println!("🧪 NaviFS MCP Tools Automated Evaluation & Integration Trace");
    println!("📍 Workspace Directory: {:?}", sample_dir);
    println!("🗄️ Database Path:       {:?}", db_path);
    println!("================================================================================");

    // -------------------------------------------------------------------------
    // STEP 1: Seed sample directory
    // -------------------------------------------------------------------------
    println!("\n[1/5] 🌱 Seeding sample directory with diverse multi-format files...");
    let seeded_files = seed_sample_directory(&sample_dir)?;
    println!("   Seeded {} files:", seeded_files.len());
    for f in &seeded_files {
        println!("   - {}", f.file_name().unwrap().to_string_lossy());
    }

    // -------------------------------------------------------------------------
    // STEP 2: Initialize Database and Indexing Pipeline
    // -------------------------------------------------------------------------
    println!("\n[2/5] ⚙️ Initializing SQLite Database, Search, and Indexing Pipeline...");
    let db = Arc::new(SqliteDatabase::new(db_path.clone())?);
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

    // Recursively scan and index seeded files
    let watch_config = WatchDirectoryConfig::new(&sample_dir);
    let scanner = RecursiveScanner::from_config(&watch_config);
    let (files_to_index, _) = scanner.scan_path(&sample_dir);

    for file_path in &files_to_index {
        pipeline.process_file(file_path).await?;
    }

    // Add explicit cross-file knowledge graph relationship for graph evaluation
    let auth_file = db
        .get_file_by_path(&seeded_files[0].to_string_lossy())
        .await?
        .expect("auth file exists");
    let arch_file = db
        .get_file_by_path(&seeded_files[1].to_string_lossy())
        .await?
        .expect("arch file exists");

    let auth_entities = db.get_entities_for_file(&auth_file.id).await?;
    let arch_entities = db.get_entities_for_file(&arch_file.id).await?;

    let auth_node_id = if let Some(e) = auth_entities.first() {
        e.id
    } else {
        let n =
            EntityNode::new("AuthService", EntityType::ClassOrStruct).with_file_id(auth_file.id);
        let id = n.id;
        db.save_entities(&[n]).await?;
        id
    };

    let arch_node_id = if let Some(e) = arch_entities.first() {
        e.id
    } else {
        let n = EntityNode::new("Security Layer", EntityType::Heading).with_file_id(arch_file.id);
        let id = n.id;
        db.save_entities(&[n]).await?;
        id
    };

    let edge = RelationEdge::new(arch_node_id, auth_node_id, RelationType::References)
        .with_weight(1.0)
        .with_properties(serde_json::json!({ "context": "Security layer implements AuthService" }));
    db.save_relations(&[edge.clone()]).await?;
    graph.add_edge(edge).await;

    let all_files = db.list_files(100, 0).await?;
    let mut total_chunks = 0;
    for f in &all_files {
        total_chunks += db.get_chunks_for_file(&f.id).await?.len();
    }
    println!(
        "   Successfully indexed {} files into {} chunks.",
        all_files.len(),
        total_chunks
    );

    // -------------------------------------------------------------------------
    // STEP 3: Setup MCP Server & Verify Initialize Header Instructions
    // -------------------------------------------------------------------------
    println!("\n[3/5] 🔌 Setting up MCP Server and Handshake (initialize & tools/list)...");
    let mcp_server = McpServer::new(db.clone(), search.clone(), graph.clone())
        .with_hybrid_engine(hybrid_search.clone());

    // Phase 1: Initialize
    let init_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(1)),
        method: "initialize".to_string(),
        params: None,
    };
    let init_resp = mcp_server.handle_request(init_req).await;
    assert!(init_resp.error.is_none(), "initialize must not error");
    let init_result = init_resp.result.expect("initialize result must exist");

    assert_eq!(
        init_result["protocolVersion"], "2024-11-05",
        "Spec: protocolVersion must be 2024-11-05"
    );
    assert_eq!(
        init_result["serverInfo"]["name"], "navifs-engine",
        "Spec: serverInfo.name must be navifs-engine"
    );

    let instructions = init_result["instructions"]
        .as_str()
        .expect("instructions must be embedded in headers");
    assert!(
        instructions.contains("NaviFS is a local-first intelligent filesystem engine"),
        "Spec: missing intro instructions"
    );
    assert!(
        instructions.contains("search"),
        "Spec: instructions must mention search tool"
    );
    assert!(
        instructions.contains("inspect"),
        "Spec: instructions must mention inspect tool"
    );
    assert!(
        instructions.contains("open"),
        "Spec: instructions must mention open tool"
    );
    assert!(
        instructions.contains("related"),
        "Spec: instructions must mention related tool"
    );
    println!("   ✅ Protocol 2024-11-05 validated. Server instructions verified in MCP headers.");

    // Phase 2: tools/list
    let tools_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(2)),
        method: "tools/list".to_string(),
        params: None,
    };
    let tools_resp = mcp_server.handle_request(tools_req).await;
    let tools_result = tools_resp.result.expect("tools/list result");
    let tools_array = tools_result["tools"].as_array().expect("tools array");
    let tool_names: Vec<String> = tools_array
        .iter()
        .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
        .collect();

    assert!(
        tool_names.contains(&"search".to_string()),
        "Must register 'search'"
    );
    assert!(
        tool_names.contains(&"inspect".to_string()),
        "Must register 'inspect'"
    );
    assert!(
        tool_names.contains(&"open".to_string()),
        "Must register 'open'"
    );
    assert!(
        tool_names.contains(&"related".to_string()),
        "Must register 'related'"
    );
    println!("   ✅ 4 Core MCP Tools Registered: {:?}", tool_names);

    // -------------------------------------------------------------------------
    // STEP 4: Complete Tool Trace Execution: search -> inspect -> related -> open
    // -------------------------------------------------------------------------
    println!("\n[4/5] 🔄 Executing Complete MCP Tool Trace...");

    // Trace 1: SEARCH
    println!("   [Trace 1/4] 🔍 Calling 'search' tool for query 'verify_jwt_token'...");
    let search_call = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(101)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "search",
            "arguments": {
                "query": "verify_jwt_token",
                "limit": 5
            }
        })),
    };
    let search_resp = mcp_server.handle_request(search_call).await;
    assert!(
        search_resp.error.is_none(),
        "search call failed: {:?}",
        search_resp.error
    );
    let search_content = &search_resp.result.as_ref().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap();
    let candidates: Vec<CandidateResult> = serde_json::from_str(search_content)
        .expect("Search output must match CandidateResult spec");

    assert!(
        !candidates.is_empty(),
        "Search must return candidate matches"
    );
    let top_candidate = &candidates[0];
    println!(
        "      Top match: {} (Score: {:.3})",
        top_candidate.filename, top_candidate.score
    );
    println!(
        "      Locator:   {}",
        top_candidate.evidence.locator_summary
    );
    println!(
        "      Snippet:   {}",
        top_candidate
            .evidence
            .snippet
            .trim()
            .lines()
            .next()
            .unwrap_or("")
    );

    // Spec assertions for search
    assert!(
        top_candidate.filename.contains("auth_service")
            || top_candidate.filename.contains("architecture"),
        "Expected top match"
    );
    assert!(
        top_candidate.score > 0.0,
        "Candidate score must be positive"
    );
    assert!(
        !top_candidate.features.lexical_score.is_nan(),
        "Features must contain valid lexical score"
    );
    assert!(
        !top_candidate.evidence.locator_summary.is_empty(),
        "Evidence must have locator summary"
    );

    let selected_file_id = top_candidate.file_id;
    let selected_path = top_candidate.path.clone();

    // Trace 2: INSPECT
    println!(
        "\n   [Trace 2/4] 🔎 Calling 'inspect' tool for target file '{}'...",
        selected_path
    );
    let inspect_call = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(102)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "inspect",
            "arguments": {
                "file_id": selected_file_id.to_string()
            }
        })),
    };
    let inspect_resp = mcp_server.handle_request(inspect_call).await;
    assert!(
        inspect_resp.error.is_none(),
        "inspect call failed: {:?}",
        inspect_resp.error
    );
    let inspect_content = &inspect_resp.result.as_ref().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap();
    let summary: MetadataSummary = serde_json::from_str(inspect_content)
        .expect("Inspect output must match MetadataSummary spec");

    println!("      File ID:     {}", summary.file_id);
    println!("      Path:        {}", summary.path);
    println!("      MIME:        {}", summary.mime_type);
    println!("      Size:        {} bytes", summary.size_bytes);
    println!("      Chunks:      {}", summary.chunk_count);
    println!(
        "      ContentHash: {}",
        summary.content_hash.as_deref().unwrap_or("none")
    );
    println!(
        "      Outline:     {} entities extracted",
        summary.outline.len()
    );

    // Spec assertions for inspect
    assert_eq!(
        summary.file_id, selected_file_id,
        "Inspect file_id must match target"
    );
    assert!(summary.size_bytes > 0, "File size must be greater than 0");
    assert!(
        summary.content_hash.is_some(),
        "Content hash (SHA-256) must be computed"
    );
    assert!(summary.chunk_count >= 1, "Must have indexed chunks");
    assert!(
        !summary.outline.is_empty(),
        "Outline must contain extracted structural symbols"
    );

    let target_entity_id = summary.outline[0].id;

    // Trace 3: RELATED
    println!(
        "\n   [Trace 3/4] 🕸️ Calling 'related' tool for entity ID '{}'...",
        target_entity_id
    );
    let related_call = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(103)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "related",
            "arguments": {
                "entity_id": target_entity_id.to_string()
            }
        })),
    };
    let related_resp = mcp_server.handle_request(related_call).await;
    assert!(
        related_resp.error.is_none(),
        "related call failed: {:?}",
        related_resp.error
    );
    let related_content = &related_resp.result.as_ref().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap();
    let graph_resp: OneHopGraphResponse = serde_json::from_str(related_content)
        .expect("Related output must match OneHopGraphResponse spec");

    println!("      Center Node: {}", graph_resp.center_entity.name);
    println!("      Outgoing:    {} edges", graph_resp.outgoing.len());
    println!("      Incoming:    {} edges", graph_resp.incoming.len());
    println!(
        "      Total Hops:  {} connections",
        graph_resp.total_connections
    );

    // Spec assertions for related
    assert_eq!(
        graph_resp.center_entity.id, target_entity_id,
        "Center entity id must match requested entity"
    );
    assert_eq!(
        graph_resp.total_connections,
        graph_resp.outgoing.len() + graph_resp.incoming.len(),
        "Total connections mismatch"
    );

    // Trace 4: OPEN (Bounded Content Range Retrieval)
    println!(
        "\n   [Trace 4/4] 📖 Calling 'open' tool for bounded lines 1..25 of '{}'...",
        selected_path
    );
    let open_call = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(104)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "open",
            "arguments": {
                "path": selected_path.clone(),
                "start_line": 1,
                "end_line": 25
            }
        })),
    };
    let open_resp = mcp_server.handle_request(open_call).await;
    assert!(
        open_resp.error.is_none(),
        "open call failed: {:?}",
        open_resp.error
    );
    let open_content = &open_resp.result.as_ref().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap();
    let bounded: BoundedContentResponse = serde_json::from_str(open_content)
        .expect("Open output must match BoundedContentResponse spec");

    println!("      Locator Summary: {}", bounded.locator_summary);
    println!(
        "      Byte Range:      {}..{}",
        bounded.byte_range.start, bounded.byte_range.end
    );
    println!("      Line Range:      {:?}", bounded.line_range);
    println!("      Retrieved Lines: {}", bounded.total_lines);
    println!("      Chunks Included: {}", bounded.chunks_included.len());
    println!(
        "      Snippet Preview: {}",
        bounded
            .content
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ")
    );

    // Spec assertions for open
    assert_eq!(
        bounded.path, selected_path,
        "Bounded path must match target"
    );
    assert!(
        bounded.locator_summary.contains("Lines 1-") || bounded.locator_summary.contains("Line 1"),
        "Locator summary must reflect line bounds"
    );
    assert!(bounded.total_lines > 0, "Total lines must be positive");
    assert!(
        !bounded.content.is_empty(),
        "Retrieved bounded content must not be empty"
    );
    assert!(
        !bounded.chunks_included.is_empty(),
        "Chunks included list must not be empty"
    );

    // -------------------------------------------------------------------------
    // STEP 5: Clean Up and Generate Report
    // -------------------------------------------------------------------------
    let _ = fs::remove_dir_all(&temp_root);

    let report = EvalReport {
        sample_dir: sample_dir.to_string_lossy().to_string(),
        files_seeded: seeded_files.len(),
        chunks_created: total_chunks,
        protocol_version: "2024-11-05".to_string(),
        server_instructions_verified: true,
        tools_registered: tool_names,
        search_verified: true,
        top_search_candidate: top_candidate.filename.clone(),
        top_search_score: top_candidate.score,
        inspect_verified: true,
        inspected_file_hash: summary.content_hash.unwrap_or_default(),
        inspected_chunk_count: summary.chunk_count,
        outline_entity_count: summary.outline.len(),
        related_verified: true,
        graph_connections: graph_resp.total_connections,
        open_verified: true,
        opened_locator: bounded.locator_summary,
        opened_lines_retrieved: bounded.total_lines,
        all_specs_passed: true,
    };

    println!("\n================================================================================");
    println!("🎉 ALL SPECIFICATIONS & TOOL TRACE VERIFIED SUCCESSFULLY!");
    println!("================================================================================");

    Ok(report)
}
