//! Model Context Protocol (MCP) server implementation for NaviFS

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};
use navifs_core::{DatabaseStore, FileId, Result, SearchProvider};
use navifs_graph::KnowledgeGraph;

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
}

impl McpServer {
    pub fn new(
        db: Arc<dyn DatabaseStore>,
        search: Arc<dyn SearchProvider>,
        graph: Arc<KnowledgeGraph>,
    ) -> Self {
        Self { db, search, graph }
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

    async fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
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
                    }
                },
                "serverInfo": {
                    "name": "navifs-engine",
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    async fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools = serde_json::json!({
            "tools": [
                {
                    "name": "search_files",
                    "description": "Lexically and semantically search indexed files and return matching snippets",
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
