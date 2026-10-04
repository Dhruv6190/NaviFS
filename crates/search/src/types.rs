//! Types and query parameters for hybrid retrieval, filtering, rank fusion, and evidence lookups

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use navifs_core::{ByteRange, ChunkId, FileId, FileIdentity, LineRange, PageRange};

/// Path-based scoping and filtering for search operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathFilter {
    /// Path prefix to restrict search to (e.g. "crates/core" or "docs/")
    pub prefix: Option<String>,
    /// Permitted file extensions without dot (e.g. ["rs", "md", "pdf"])
    pub extensions: Option<Vec<String>>,
    /// Custom glob pattern substring (e.g. "test", "src")
    pub glob_pattern: Option<String>,
}

impl PathFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = Some(extensions.into_iter().map(|e| e.to_lowercase()).collect());
        self
    }

    pub fn with_glob(mut self, pattern: impl Into<String>) -> Self {
        self.glob_pattern = Some(pattern.into());
        self
    }

    /// Evaluates whether a given file path satisfies the filter rules
    pub fn matches_path(&self, path_str: &str) -> bool {
        let normalized = path_str.replace('\\', "/");

        // 1. Prefix check
        if let Some(ref prefix) = self.prefix {
            let norm_prefix = prefix.replace('\\', "/");
            if !normalized.starts_with(&norm_prefix) {
                return false;
            }
        }

        // 2. Extension check
        if let Some(ref allowed_exts) = self.extensions {
            let ext = std::path::Path::new(&normalized)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !allowed_exts.iter().any(|allowed| allowed.eq_ignore_ascii_case(&ext)) {
                return false;
            }
        }

        // 3. Glob pattern substring check
        if let Some(ref glob) = self.glob_pattern {
            if !normalized.contains(glob) {
                return false;
            }
        }

        true
    }
}

/// Metadata-based filter attributes for refining candidate documents
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataFilter {
    /// Permitted MIME types (e.g. ["text/plain", "application/pdf"])
    pub mime_types: Option<Vec<String>>,
    /// Minimum file modification timestamp
    pub min_modified: Option<DateTime<Utc>>,
    /// Maximum file modification timestamp
    pub max_modified: Option<DateTime<Utc>>,
    /// Minimum file size in bytes
    pub min_size_bytes: Option<u64>,
    /// Maximum file size in bytes
    pub max_size_bytes: Option<u64>,
}

impl MetadataFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mime_types(mut self, mimes: Vec<String>) -> Self {
        self.mime_types = Some(mimes);
        self
    }

    pub fn with_time_range(mut self, min: Option<DateTime<Utc>>, max: Option<DateTime<Utc>>) -> Self {
        self.min_modified = min;
        self.max_modified = max;
        self
    }

    pub fn with_size_range(mut self, min_bytes: Option<u64>, max_bytes: Option<u64>) -> Self {
        self.min_size_bytes = min_bytes;
        self.max_size_bytes = max_bytes;
        self
    }

    /// Evaluates whether a file identity satisfies the metadata criteria
    pub fn matches_file(&self, file: &FileIdentity) -> bool {
        // 1. MIME types
        if let Some(ref allowed_mimes) = self.mime_types {
            let mime_str = file.mime_type.as_str();
            if !allowed_mimes.iter().any(|m| m.eq_ignore_ascii_case(mime_str)) {
                return false;
            }
        }

        // 2. Modified time bounds
        if let Some(min_t) = self.min_modified {
            if file.modified_at < min_t {
                return false;
            }
        }
        if let Some(max_t) = self.max_modified {
            if file.modified_at > max_t {
                return false;
            }
        }

        // 3. File size bounds
        if let Some(min_s) = self.min_size_bytes {
            if file.size_bytes < min_s {
                return false;
            }
        }
        if let Some(max_s) = self.max_size_bytes {
            if file.size_bytes > max_s {
                return false;
            }
        }

        true
    }
}

/// Feature weighting configurations for the composite reranker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerWeights {
    /// Lexical relevance weight (FTS5 BM25 + rank fusion score)
    pub lexical_weight: f32,
    /// Path and filename similarity weight
    pub path_similarity_weight: f32,
    /// Recency/freshness decay weight
    pub freshness_weight: f32,
    /// Structural extraction quality weight (line/page bounds, content completeness)
    pub quality_weight: f32,
}

impl Default for RerankerWeights {
    fn default() -> Self {
        Self {
            lexical_weight: 0.45,
            path_similarity_weight: 0.25,
            freshness_weight: 0.15,
            quality_weight: 0.15,
        }
    }
}

impl RerankerWeights {
    pub fn balanced() -> Self {
        Self::default()
    }

    pub fn lexical_heavy() -> Self {
        Self {
            lexical_weight: 0.70,
            path_similarity_weight: 0.15,
            freshness_weight: 0.05,
            quality_weight: 0.10,
        }
    }

    pub fn code_focused() -> Self {
        Self {
            lexical_weight: 0.40,
            path_similarity_weight: 0.35,
            freshness_weight: 0.10,
            quality_weight: 0.15,
        }
    }
}

/// Breakdown of individual feature vector components evaluated during reranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerFeatures {
    pub lexical_score: f32,
    pub path_similarity: f32,
    pub freshness_score: f32,
    pub extraction_quality: f32,
    pub composite_score: f32,
}

/// Evidence lookup structure containing exact line/page locators and content preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLookup {
    pub chunk_id: Option<ChunkId>,
    pub file_id: FileId,
    pub filename: String,
    pub path: String,
    pub byte_range: ByteRange,
    pub line_range: Option<LineRange>,
    pub page_range: Option<PageRange>,
    pub snippet: String,
    pub locator_summary: String,
    pub content: String,
}

/// Final scored candidate returned by hybrid retrieval and feature reranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateResult {
    pub file_id: FileId,
    pub chunk_id: Option<ChunkId>,
    pub filename: String,
    pub path: String,
    pub mime_type: String,
    pub score: f32,
    pub features: RerankerFeatures,
    pub evidence: EvidenceLookup,
}

/// Query object for hybrid retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchQuery {
    pub query: String,
    pub path_filter: Option<PathFilter>,
    pub metadata_filter: Option<MetadataFilter>,
    pub limit: usize,
    pub weights: RerankerWeights,
}

impl HybridSearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            path_filter: None,
            metadata_filter: None,
            limit: 10,
            weights: RerankerWeights::default(),
        }
    }

    pub fn with_path_filter(mut self, filter: PathFilter) -> Self {
        self.path_filter = Some(filter);
        self
    }

    pub fn with_metadata_filter(mut self, filter: MetadataFilter) -> Self {
        self.metadata_filter = Some(filter);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    pub fn with_weights(mut self, weights: RerankerWeights) -> Self {
        self.weights = weights;
        self
    }
}
