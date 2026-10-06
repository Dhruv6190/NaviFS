//! `search` tool: Hybrid retrieval combining SQLite FTS5, path/metadata filters,
//! aggressive local rank fusion, and feature vector reranking with evidence locators.

use chrono::{DateTime, Utc};
use navifs_core::{ByteRange, ChunkId, FileId, SearchProvider};
use navifs_search::{
    CandidateResult, EvidenceBuilder, EvidenceLookup, HybridSearchEngine, HybridSearchQuery,
    MetadataFilter, PathFilter, RerankerFeatures,
};
use serde::Deserialize;
use serde_json::Value;

/// Nested filters object according to MCP Contract §7.2
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchFilters {
    pub path_prefix: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub mime_types: Option<Vec<String>>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub path_prefix: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub mime_types: Option<Vec<String>>,
    pub filters: Option<SearchFilters>,
    pub limit: Option<usize>,
}

pub struct SearchTool;

impl SearchTool {
    pub fn schema() -> Value {
        serde_json::json!({
            "name": "search",
            "description": "Execute hybrid retrieval across the filesystem combining SQLite FTS5 lexical matching, metadata/path filters, aggressive rank fusion, and feature vector reranking (lexical, path similarity, freshness, extraction quality) returning top-K candidates with evidence locators",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query terms or phrase"
                    },
                    "filters": {
                        "type": "object",
                        "description": "Optional structured filters for narrowing search candidate space",
                        "properties": {
                            "path_prefix": {
                                "type": "string",
                                "description": "Optional directory or path prefix (e.g. 'crates/core' or 'docs/')"
                            },
                            "extensions": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional list of allowed file extensions without dot (e.g. ['rs', 'md', 'pdf', 'xlsx'])"
                            },
                            "mime_types": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional list of allowed MIME types (e.g. ['application/pdf', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'])"
                            },
                            "modified_after": {
                                "type": "string",
                                "description": "Optional ISO 8601 timestamp string for lower bound on modification time (e.g. '2026-09-01T00:00:00Z')"
                            },
                            "modified_before": {
                                "type": "string",
                                "description": "Optional ISO 8601 timestamp string for upper bound on modification time (e.g. '2026-10-01T00:00:00Z')"
                            }
                        }
                    },
                    "path_prefix": {
                        "type": "string",
                        "description": "Optional directory or path prefix to restrict search scope (e.g. 'crates/core' or 'docs/')"
                    },
                    "extensions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of allowed file extensions without dot (e.g. ['rs', 'md', 'pdf'])"
                    },
                    "mime_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of allowed MIME types (e.g. ['application/pdf', 'text/markdown'])"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of top-K candidates to return (default 10)"
                    }
                },
                "required": ["query"]
            }
        })
    }

    pub async fn execute(
        hybrid_engine: Option<&HybridSearchEngine>,
        search_provider: &dyn SearchProvider,
        args: SearchArgs,
    ) -> Result<Vec<CandidateResult>, String> {
        let limit = args.limit.unwrap_or(10).max(1);
        let mut query = HybridSearchQuery::new(&args.query).with_limit(limit);

        // Merge flat arguments with nested filters object (§7.2)
        let path_prefix = args
            .filters
            .as_ref()
            .and_then(|f| f.path_prefix.clone())
            .or(args.path_prefix);
        let extensions = args
            .filters
            .as_ref()
            .and_then(|f| f.extensions.clone())
            .or(args.extensions);
        let mime_types = args
            .filters
            .as_ref()
            .and_then(|f| f.mime_types.clone())
            .or(args.mime_types);

        let mut path_filter = PathFilter::new();
        let mut has_path_filter = false;

        if let Some(prefix) = path_prefix {
            path_filter = path_filter.with_prefix(prefix);
            has_path_filter = true;
        }

        if let Some(exts) = extensions {
            if !exts.is_empty() {
                path_filter = path_filter.with_extensions(exts);
                has_path_filter = true;
            }
        }

        if has_path_filter {
            query = query.with_path_filter(path_filter);
        }

        let mut meta_filter = MetadataFilter::new();
        let mut has_meta_filter = false;

        if let Some(mimes) = mime_types {
            if !mimes.is_empty() {
                meta_filter = meta_filter.with_mime_types(mimes);
                has_meta_filter = true;
            }
        }

        if let Some(ref f) = args.filters {
            let min_mod: Option<DateTime<Utc>> = f.modified_after.as_ref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            });
            let max_mod: Option<DateTime<Utc>> = f.modified_before.as_ref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            });

            if min_mod.is_some() || max_mod.is_some() {
                meta_filter = meta_filter.with_time_range(min_mod, max_mod);
                has_meta_filter = true;
            }
        }

        if has_meta_filter {
            query = query.with_metadata_filter(meta_filter);
        }

        if let Some(engine) = hybrid_engine {
            engine.search_hybrid(query).await.map_err(|e| e.to_string())
        } else {
            let hits = search_provider
                .search(&args.query, limit)
                .await
                .map_err(|e| e.to_string())?;

            let candidates = hits
                .into_iter()
                .map(|h| {
                    let file_id = FileId::from_uuid(h.file_id);
                    let chunk_id = Some(ChunkId::from_uuid(h.chunk_id));
                    let filename = std::path::Path::new(&h.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let byte_range = ByteRange::new(0, h.content.len() as u64);
                    let locator_summary =
                        EvidenceBuilder::format_locator_summary(&filename, None, None);

                    CandidateResult {
                        file_id,
                        chunk_id,
                        filename: filename.clone(),
                        path: h.path.clone(),
                        mime_type: "text/plain".to_string(),
                        score: h.score,
                        features: RerankerFeatures {
                            lexical_score: h.score,
                            path_similarity: 1.0,
                            freshness_score: 1.0,
                            extraction_quality: 1.0,
                            composite_score: h.score,
                        },
                        evidence: EvidenceLookup {
                            chunk_id,
                            file_id,
                            filename,
                            path: h.path,
                            byte_range,
                            line_range: None,
                            page_range: None,
                            snippet: h.content.clone(),
                            locator_summary,
                            content: h.content,
                        },
                    }
                })
                .collect();

            Ok(candidates)
        }
    }
}
