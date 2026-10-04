//! Hybrid retrieval and lexical search engine for NaviFS
//!
//! Combines SQLite FTS5 lexical matching, path and metadata filtering, aggressive local rank fusion,
//! multi-dimensional feature vector reranking (lexical, path similarity, freshness, extraction quality),
//! and evidence lookups with line and page bounds.

pub mod evidence;
pub mod fusion;
pub mod hybrid;
pub mod reranker;
pub mod types;

pub use evidence::EvidenceBuilder;
pub use fusion::{AggressiveRankFusion, FusedCandidate, RankFusionConfig};
pub use hybrid::HybridSearchEngine;
pub use reranker::FeatureVectorReranker;
pub use types::{
    CandidateResult, EvidenceLookup, HybridSearchQuery, MetadataFilter, PathFilter,
    RerankerFeatures, RerankerWeights,
};

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, info};
use navifs_core::{FileChunk, FileId, Result, SearchHit, SearchProvider};

/// Inverted-index entry for indexed terms
#[derive(Debug, Clone)]
struct IndexEntry {
    chunk_id: uuid::Uuid,
    file_id: uuid::Uuid,
    term_frequency: usize,
    chunk_index: u32,
    #[allow(dead_code)]
    preview: String,
}

/// Tokenizer that converts raw text into normalized search terms
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.len() > 1)
        .map(|t| t.to_lowercase())
        .collect()
}

/// In-memory lexical search engine with inverted-index and BM25 term weighting
pub struct LexicalSearchEngine {
    // term -> list of occurrences
    inverted_index: Arc<RwLock<HashMap<String, Vec<IndexEntry>>>>,
    // chunk_id -> raw chunk content
    chunk_store: Arc<RwLock<HashMap<uuid::Uuid, (uuid::Uuid, u32, String)>>>,
}

impl Default for LexicalSearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LexicalSearchEngine {
    pub fn new() -> Self {
        Self {
            inverted_index: Arc::new(RwLock::new(HashMap::new())),
            chunk_store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SearchProvider for LexicalSearchEngine {
    async fn index_chunks(&self, chunks: &[FileChunk]) -> Result<()> {
        let mut index = self.inverted_index.write().await;
        let mut store = self.chunk_store.write().await;

        for chunk in chunks {
            let chunk_uuid = *chunk.id.as_uuid();
            let file_uuid = *chunk.file_id.as_uuid();

            // Store chunk data
            store.insert(chunk_uuid, (file_uuid, chunk.chunk_index, chunk.content.clone()));

            // Tokenize content
            let tokens = tokenize(&chunk.content);
            let mut tf_map: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *tf_map.entry(token).or_insert(0) += 1;
            }

            let preview = chunk.content.chars().take(200).collect::<String>();

            // Insert into inverted index
            for (term, freq) in tf_map {
                let entry = IndexEntry {
                    chunk_id: chunk_uuid,
                    file_id: file_uuid,
                    term_frequency: freq,
                    chunk_index: chunk.chunk_index,
                    preview: preview.clone(),
                };
                index.entry(term).or_default().push(entry);
            }
        }

        debug!("Indexed {} chunks into lexical search index", chunks.len());
        Ok(())
    }

    async fn remove_file_from_index(&self, file_id: &FileId) -> Result<()> {
        let file_uuid = *file_id.as_uuid();
        let mut index = self.inverted_index.write().await;
        let mut store = self.chunk_store.write().await;

        store.retain(|_, (f_id, _, _)| *f_id != file_uuid);

        for entries in index.values_mut() {
            entries.retain(|e| e.file_id != file_uuid);
        }

        debug!("Removed file {} from search index", file_id);
        Ok(())
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Ok(Vec::new());
        }

        let index = self.inverted_index.read().await;
        let store = self.chunk_store.read().await;

        let mut scores: HashMap<uuid::Uuid, (f32, uuid::Uuid, u32)> = HashMap::new();
        let total_docs = store.len().max(1) as f32;

        for term in &query_terms {
            if let Some(entries) = index.get(term) {
                let doc_freq = entries.len() as f32;
                let idf = (total_docs / (1.0 + doc_freq)).ln() + 1.0;

                for entry in entries {
                    let tf = entry.term_frequency as f32;
                    let term_score = tf * idf;
                    let acc = scores.entry(entry.chunk_id).or_insert((0.0, entry.file_id, entry.chunk_index));
                    acc.0 += term_score;
                }
            }
        }

        let mut ranked: Vec<(uuid::Uuid, (f32, uuid::Uuid, u32))> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap_or(std::cmp::Ordering::Equal));

        let mut hits = Vec::new();
        for (chunk_id, (score, file_id, chunk_index)) in ranked.into_iter().take(limit) {
            let content = store
                .get(&chunk_id)
                .map(|(_, _, c)| c.clone())
                .unwrap_or_default();

            hits.push(SearchHit {
                chunk_id,
                file_id,
                score,
                content,
                path: String::new(),
                chunk_index,
            });
        }

        info!("Search for '{}' produced {} hits", query, hits.len());
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use navifs_core::{ByteRange, ChunkType, ContentHash, DatabaseStore, FileIdentity, LineRange, PageRange};
    use navifs_database::SqliteDatabase;

    #[test]
    fn test_path_filter_matching() {
        let filter = PathFilter::new()
            .with_prefix("crates/core")
            .with_extensions(vec!["rs".to_string(), "md".to_string()]);

        assert!(filter.matches_path("crates/core/src/lib.rs"));
        assert!(filter.matches_path("crates/core/README.md"));
        assert!(!filter.matches_path("crates/database/src/lib.rs"));
        assert!(!filter.matches_path("crates/core/binary.exe"));
    }

    #[test]
    fn test_metadata_filter_matching() {
        let file = FileIdentity::new(Path::new("test.pdf"), 5000, chrono::Utc::now());
        let filter = MetadataFilter::new()
            .with_size_range(Some(1000), Some(10000))
            .with_mime_types(vec!["application/pdf".to_string()]);

        assert!(filter.matches_file(&file));

        let small_filter = MetadataFilter::new().with_size_range(Some(10000), None);
        assert!(!small_filter.matches_file(&file));
    }

    #[test]
    fn test_evidence_locator_summary_formatting() {
        let line_range = Some(LineRange::new(10, 25));
        let page_range = Some(PageRange::new(3, 3));

        let summary = EvidenceBuilder::format_locator_summary("report.pdf", line_range, page_range);
        assert_eq!(summary, "report.pdf:Page 3 (Lines 10-25)");

        let code_summary = EvidenceBuilder::format_locator_summary("main.rs", line_range, None);
        assert_eq!(code_summary, "main.rs:Lines 10-25");
    }

    #[tokio::test]
    async fn test_hybrid_search_end_to_end() {
        let db = Arc::new(SqliteDatabase::in_memory().expect("Failed in-memory DB"));
        db.initialize().await.expect("Migrations failed");

        // 1. Insert test file
        let path = Path::new("crates/indexer/src/scanner.rs");
        let mut file = FileIdentity::new(path, 4096, chrono::Utc::now());
        file = file.with_hash(ContentHash::from_bytes(b"content for hash"));
        db.upsert_file(&file).await.unwrap();

        // 2. Insert test chunk
        let chunk = FileChunk::new(
            file.id,
            0,
            ChunkType::CodeBlock { language: "rust".to_string() },
            ByteRange::new(0, 200),
            Some(LineRange::new(20, 50)),
            "pub struct RecursiveScanner with notify watcher and SHA256 detection".to_string(),
        );
        db.save_chunks(&[chunk]).await.unwrap();

        // 3. Perform Hybrid Search
        let engine = HybridSearchEngine::new(db);
        let query = HybridSearchQuery::new("RecursiveScanner")
            .with_path_filter(PathFilter::new().with_extensions(vec!["rs".to_string()]))
            .with_limit(5);

        let results = engine.search_hybrid(query).await.unwrap();
        assert!(!results.is_empty(), "Expected candidate result from hybrid search");
        let top = &results[0];
        assert_eq!(top.file_id, file.id);
        assert!(top.score > 0.0);
        assert!(top.evidence.line_range.is_some());
        assert_eq!(top.evidence.line_range.unwrap().start_line, 20);
        assert_eq!(top.evidence.line_range.unwrap().end_line, 50);
        assert!(top.evidence.locator_summary.contains("Lines 20-50"));
    }

    #[tokio::test]
    async fn test_hybrid_search_with_vector_channel() {
        let db = Arc::new(SqliteDatabase::in_memory().expect("Failed in-memory DB"));
        db.initialize().await.expect("Migrations failed");

        let path = Path::new("crates/core/src/vector_test.rs");
        let mut file = FileIdentity::new(path, 1024, chrono::Utc::now());
        file = file.with_hash(ContentHash::from_bytes(b"vector test"));
        db.upsert_file(&file).await.unwrap();

        let chunk = FileChunk::new(
            file.id,
            0,
            ChunkType::CodeBlock { language: "rust".to_string() },
            ByteRange::new(0, 100),
            Some(LineRange::new(1, 10)),
            "fn vector_search() {}".to_string(),
        );
        db.save_chunks(&[chunk.clone()]).await.unwrap();

        let embedding = navifs_core::EmbeddingRecord::new(
            file.id,
            Some(chunk.id),
            "text-embedding-3-small",
            vec![0.5, 0.5, 0.5, 0.5],
        );
        db.save_embeddings(&[embedding]).await.unwrap();

        let engine = HybridSearchEngine::new(db);
        let query = HybridSearchQuery::new("vector_search")
            .with_vector(vec![0.5, 0.5, 0.5, 0.5], "text-embedding-3-small")
            .with_limit(5);

        let results = engine.search_hybrid(query).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_id, file.id);
    }
}
