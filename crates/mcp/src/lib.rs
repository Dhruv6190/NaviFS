//! Model Context Protocol (MCP) server implementation for NaviFS
//!
//! Provides the four core NaviFS tools:
//! - `search`: Hybrid retrieval combining SQLite FTS5 matches, path/metadata filters, aggressive rank fusion, and feature vector reranking with evidence locators.
//! - `inspect`: Structural metadata summaries, chunk statistics, page counts, content hashes, and document outline entities.
//! - `open`: Bounded content range retrieval (by line range, page range, byte bounds, or chunk index) with exact index locators.
//! - `related`: One-to-one hop knowledge graph relationship traversal discovering adjacent connected entities and directional edges.
//!
//! Server instructions are embedded in the MCP handshake headers (`initialize` response).

pub mod tools;

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use navifs_core::{DatabaseStore, FileId, Result, SearchProvider};
use navifs_graph::KnowledgeGraph;
use navifs_search::HybridSearchEngine;

pub use tools::{
    BoundedContentResponse, ConnectedEntity, InspectArgs, InspectTool, MetadataSummary,
    OneHopGraphResponse, OpenArgs, OpenTool, RelatedArgs, RelatedTool, SearchArgs, SearchTool,
};

/// JSON-RPC 2.0 Request payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// NaviFS MCP Server
pub struct McpServer {
    db: Arc<dyn DatabaseStore>,
    search: Arc<dyn SearchProvider>,
    graph: Arc<KnowledgeGraph>,
    hybrid_search: Option<Arc<HybridSearchEngine>>,
}

impl McpServer {
    pub fn new(
        db: Arc<dyn DatabaseStore>,
        search: Arc<dyn SearchProvider>,
        graph: Arc<KnowledgeGraph>,
    ) -> Self {
        Self {
            db,
            search,
            graph,
            hybrid_search: None,
        }
    }

    pub fn with_hybrid_engine(mut self, hybrid: Arc<HybridSearchEngine>) -> Self {
        self.hybrid_search = Some(hybrid);
        self
    }

    pub fn hybrid_engine(&self) -> Option<&HybridSearchEngine> {
        self.hybrid_search.as_deref()
    }

    pub fn db(&self) -> &Arc<dyn DatabaseStore> {
        &self.db
    }

    pub fn graph(&self) -> &Arc<KnowledgeGraph> {
        &self.graph
    }

    /// Process a single incoming JSON-RPC request line
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id).await,
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({})),
                error: None,
            },
            "tools/list" => self.handle_tools_list(request.id).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            },
        }
    }

    /// Handle initialize request with server instructions embedded in MCP headers
    async fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        let instructions = concat!(
            "NaviFS is a local-first intelligent filesystem engine exposed via Model Context Protocol (MCP).\n\n",
            "Core Retrieval & Inspection Pipeline:\n",
            "1. `search`: Execute hybrid retrieval combining SQLite FTS5 lexical matching, metadata/path filters, ",
            "aggressive rank fusion (RRF k=20.0), and feature vector reranking (lexical, path similarity, freshness, quality) ",
            "returning top-K candidates with exact evidence locators.\n",
            "2. `inspect`: Generate structural metadata summaries, chunk statistics, page counts, content hashes (SHA-256), ",
            "and document outline entities (headings, sections, functions).\n",
            "3. `open`: Retrieve bounded content ranges by lines (e.g. L10-L40), pages, byte offsets, or chunk index ",
            "with precise index locator summaries.\n",
            "4. `related`: Traverse one-to-one hop knowledge graph relationships discovering adjacent entity nodes ",
            "and directional edges (Contains, Defines, References, Imports).\n\n",
            "Guidelines for AI Agents:\n",
            "- Use `search` first to discover relevant candidate files and snippets.\n",
            "- Use `inspect` to examine document outline, structure, and chunk metrics before reading large files.\n",
            "- Use `open` with targeted line or page bounds to read only the necessary content range.\n",
            "- Use `related` to navigate relationships across entities, files, and modules."
        );

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    },
                    "resources": {
                        "subscribe": false,
                        "listChanged": false
                    },
                    "prompts": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "navifs-engine",
                    "version": "0.1.0"
                },
                "instructions": instructions
            })),
            error: None,
        }
    }

    /// Expose schemas for the 4 core MCP tools: search, inspect, open, related
    async fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools = serde_json::json!({
            "tools": [
                SearchTool::schema(),
                InspectTool::schema(),
                OpenTool::schema(),
                RelatedTool::schema(),
                // Backward-compatibility aliases
                {
                    "name": "hybrid_search",
                    "description": "Alias for search: Execute hybrid retrieval across the filesystem",
                    "inputSchema": SearchTool::schema()["inputSchema"].clone()
                },
                {
                    "name": "search_files",
                    "description": "Lexically search indexed files and return matching snippets",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "The search keywords or query phrase" },
                            "limit": { "type": "integer", "description": "Maximum number of results (default 10)" }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "get_file_context",
                    "description": "Retrieve full chunks and metadata for a specific file by its file_id or path",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "file_id": { "type": "string", "description": "UUID of the file" },
                            "path": { "type": "string", "description": "Path to file" }
                        }
                    }
                },
                {
                    "name": "list_files",
                    "description": "List tracked files in the workspace with metadata",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "limit": { "type": "integer", "description": "Limit of files" },
                            "offset": { "type": "integer", "description": "Offset" }
                        }
                    }
                }
            ]
        });

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(tools),
            error: None,
        }
    }

    /// Dispatch tool execution calls to the respective tool handlers
    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                        data: None,
                    }),
                }
            }
        };

        let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

        match tool_name {
            // Core Tool 1: search (and hybrid_search alias)
            "search" | "hybrid_search" => {
                let search_args: SearchArgs = match serde_json::from_value(arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{ "type": "text", "text": format!("Invalid search arguments: {}", e) }],
                                "isError": true
                            })),
                            error: None,
                        }
                    }
                };

                match SearchTool::execute(self.hybrid_search.as_deref(), self.search.as_ref(), search_args).await {
                    Ok(candidates) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&candidates).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": format!("Search error: {}", e) }],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }

            // Core Tool 2: inspect
            "inspect" => {
                let inspect_args: InspectArgs = match serde_json::from_value(arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{ "type": "text", "text": format!("Invalid inspect arguments: {}", e) }],
                                "isError": true
                            })),
                            error: None,
                        }
                    }
                };

                match InspectTool::execute(self.db.as_ref(), inspect_args).await {
                    Ok(summary) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&summary).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": format!("Inspect error: {}", e) }],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }

            // Core Tool 3: open
            "open" => {
                let open_args: OpenArgs = match serde_json::from_value(arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{ "type": "text", "text": format!("Invalid open arguments: {}", e) }],
                                "isError": true
                            })),
                            error: None,
                        }
                    }
                };

                match OpenTool::execute(self.db.as_ref(), open_args).await {
                    Ok(content_resp) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&content_resp).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": format!("Open error: {}", e) }],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }

            // Core Tool 4: related
            "related" => {
                let related_args: RelatedArgs = match serde_json::from_value(arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        return JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [{ "type": "text", "text": format!("Invalid related arguments: {}", e) }],
                                "isError": true
                            })),
                            error: None,
                        }
                    }
                };

                match RelatedTool::execute(self.db.as_ref(), self.graph.as_ref(), related_args).await {
                    Ok(graph_resp) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&graph_resp).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [{ "type": "text", "text": format!("Related error: {}", e) }],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }

            // Legacy tools for backward-compatibility
            "search_files" => {
                let query = arguments.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = arguments.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;

                match self.search.search(query, limit).await {
                    Ok(hits) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&hits).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32000,
                            message: e.to_string(),
                            data: None,
                        }),
                    },
                }
            }

            "list_files" => {
                let limit = arguments.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize;
                let offset = arguments.get("offset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;

                match self.db.list_files(limit, offset).await {
                    Ok(files) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&files).unwrap_or_default()
                                }
                            ]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32000,
                            message: e.to_string(),
                            data: None,
                        }),
                    },
                }
            }

            "get_file_context" => {
                let file_res = if let Some(id_str) = arguments.get("file_id").and_then(|i| i.as_str()) {
                    match FileId::parse(id_str) {
                        Ok(fid) => self.db.get_file(&fid).await,
                        Err(e) => Err(navifs_core::NaviError::InvalidId(e.to_string())),
                    }
                } else if let Some(path_str) = arguments.get("path").and_then(|p| p.as_str()) {
                    self.db.get_file_by_path(path_str).await
                } else {
                    Ok(None)
                };

                match file_res {
                    Ok(Some(file)) => {
                        let chunks = self.db.get_chunks_for_file(&file.id).await.unwrap_or_default();
                        JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: Some(serde_json::json!({
                                "content": [
                                    {
                                        "type": "text",
                                        "text": serde_json::to_string_pretty(&serde_json::json!({
                                            "file": file,
                                            "chunks": chunks
                                        })).unwrap_or_default()
                                    }
                                ]
                            })),
                            error: None,
                        }
                    }
                    Ok(None) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(serde_json::json!({
                            "content": [
                                { "type": "text", "text": "File not found" }
                            ],
                            "isError": true
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32000,
                            message: e.to_string(),
                            data: None,
                        }),
                    },
                }
            }

            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Unknown tool: {}", tool_name),
                    data: None,
                }),
            },
        }
    }

    /// Run the server loop reading JSON-RPC lines from standard input and writing to standard output
    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting NaviFS MCP Server over standard I/O (stdio)...");
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(line) {
                Ok(req) => {
                    let resp = self.handle_request(req).await;
                    if let Ok(resp_str) = serde_json::to_string(&resp) {
                        let _ = stdout.write_all(resp_str.as_bytes()).await;
                        let _ = stdout.write_all(b"\n").await;
                        let _ = stdout.flush().await;
                    }
                }
                Err(err) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", err),
                            data: None,
                        }),
                    };
                    if let Ok(resp_str) = serde_json::to_string(&err_resp) {
                        let _ = stdout.write_all(resp_str.as_bytes()).await;
                        let _ = stdout.write_all(b"\n").await;
                        let _ = stdout.flush().await;
                    }
                }
            }
        }

        info!("NaviFS MCP Server stdio loop terminated.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use navifs_core::{
        ByteRange, ChunkType, ContentHash, EntityNode, EntityType, FileChunk, FileIdentity,
        LineRange, RelationEdge, RelationType,
    };
    use navifs_database::SqliteDatabase;
    use navifs_search::LexicalSearchEngine;

    async fn create_test_mcp_server() -> McpServer {
        let db = Arc::new(SqliteDatabase::in_memory().expect("create test db"));
        db.initialize().await.expect("init db");

        let search = Arc::new(LexicalSearchEngine::new());
        let graph = Arc::new(KnowledgeGraph::new());

        // Populate test data
        let test_path = std::path::Path::new("src/main.rs");
        let mut identity = FileIdentity::new(test_path, 1024, chrono::Utc::now());
        identity = identity.with_hash(ContentHash::new("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));

        db.upsert_file(&identity).await.expect("upsert file");

        let chunk = FileChunk::new(
            identity.id,
            0,
            ChunkType::Generic,
            ByteRange::new(0, 120),
            Some(LineRange::new(1, 3)),
            "fn main() {\n    println!(\"Hello from NaviFS MCP server!\");\n}".to_string(),
        );

        db.save_chunks(&[chunk]).await.expect("save chunk");

        // Add entity and relation
        let entity = EntityNode::new("main", EntityType::Function)
            .with_file_id(identity.id)
            .with_properties(serde_json::json!({ "visibility": "pub" }));
        let entity_id = entity.id;
        db.save_entities(&[entity]).await.expect("save entity");

        let target_entity = EntityNode::new("println", EntityType::Function)
            .with_file_id(identity.id)
            .with_properties(serde_json::json!({ "kind": "macro" }));
        let target_id = target_entity.id;
        db.save_entities(&[target_entity]).await.expect("save target entity");

        let edge = RelationEdge::new(entity_id, target_id, RelationType::References)
            .with_weight(1.0);
        db.save_relations(&[edge]).await.expect("save relation");

        let hybrid = Arc::new(HybridSearchEngine::new(db.clone()));
        McpServer::new(db, search, graph).with_hybrid_engine(hybrid)
    }

    #[tokio::test]
    async fn test_initialize_embeds_server_instructions() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(1)),
            method: "initialize".to_string(),
            params: None,
        };

        let resp = server.handle_request(req).await;
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(Value::from(1)));
        assert!(resp.error.is_none());

        let result = resp.result.expect("result must be present");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "navifs-engine");

        // Verify server instructions embedded in MCP headers
        let instructions = result["instructions"].as_str().expect("instructions must be string");
        assert!(instructions.contains("NaviFS is a local-first intelligent filesystem engine"));
        assert!(instructions.contains("search"));
        assert!(instructions.contains("inspect"));
        assert!(instructions.contains("open"));
        assert!(instructions.contains("related"));
    }

    #[tokio::test]
    async fn test_tools_list_registers_four_core_tools() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());

        let result = resp.result.expect("tools list result");
        let tools = result["tools"].as_array().expect("tools array");
        let tool_names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();

        assert!(tool_names.contains(&"search"), "Must contain 'search' tool");
        assert!(tool_names.contains(&"inspect"), "Must contain 'inspect' tool");
        assert!(tool_names.contains(&"open"), "Must contain 'open' tool");
        assert!(tool_names.contains(&"related"), "Must contain 'related' tool");
    }

    #[tokio::test]
    async fn test_search_tool_execution() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(3)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "search",
                "arguments": {
                    "query": "Hello from NaviFS",
                    "limit": 5
                }
            })),
        };

        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.expect("search result");
        assert!(result["content"].is_array());
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("main.rs") || text.contains("Hello from NaviFS") || text.contains("score"));
    }

    #[tokio::test]
    async fn test_inspect_tool_metadata_summary() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(4)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "inspect",
                "arguments": {
                    "path": "src/main.rs"
                }
            })),
        };

        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.expect("inspect result");
        let text = result["content"][0]["text"].as_str().unwrap();
        let summary: Value = serde_json::from_str(text).expect("valid json summary");

        assert_eq!(summary["path"], "src/main.rs");
        assert_eq!(summary["filename"], "main.rs");
        assert_eq!(summary["mime_type"], "application/x-rust");
        assert_eq!(summary["chunk_count"], 1);
        assert_eq!(summary["line_count"], 3);
        assert!(summary["content_hash"].as_str().is_some());
        assert!(!summary["outline"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_open_tool_bounded_content_range() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(5)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "open",
                "arguments": {
                    "path": "src/main.rs",
                    "start_line": 1,
                    "end_line": 3
                }
            })),
        };

        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.expect("open result");
        let text = result["content"][0]["text"].as_str().unwrap();
        let opened: Value = serde_json::from_str(text).expect("valid bounded content response");

        assert_eq!(opened["path"], "src/main.rs");
        assert!(opened["locator_summary"].as_str().unwrap().contains("main.rs:Lines 1-3"));
        assert!(opened["content"].as_str().unwrap().contains("println!"));
        assert_eq!(opened["total_lines"], 3);
    }

    #[tokio::test]
    async fn test_related_tool_one_hop_graph() {
        let server = create_test_mcp_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(6)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "related",
                "arguments": {
                    "path": "src/main.rs"
                }
            })),
        };

        let resp = server.handle_request(req).await;
        assert!(resp.error.is_none());
        let result = resp.result.expect("related result");
        let text = result["content"][0]["text"].as_str().unwrap();
        let graph_resp: Value = serde_json::from_str(text).expect("valid one hop graph response");

        assert_eq!(graph_resp["center_entity"]["name"], "main");
        assert_eq!(graph_resp["total_connections"], 1);
        let outgoing = graph_resp["outgoing"].as_array().expect("outgoing edges");
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0]["relation_type"], "References");
        assert_eq!(outgoing[0]["entity"]["name"], "println");
    }
}
