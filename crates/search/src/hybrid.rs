//! Hybrid retrieval engine combining SQLite FTS5 lexical search, metadata/path filters,
//! aggressive local rank fusion, and feature vector reranking.

use crate::evidence::EvidenceBuilder;
use crate::fusion::AggressiveRankFusion;
use crate::reranker::FeatureVectorReranker;
use crate::types::{CandidateResult, HybridSearchQuery};
use async_trait::async_trait;
use navifs_core::{
    ChunkId, DatabaseStore, Embedder, FileChunk, FileId, FileIdentity, Result, SearchHit,
    SearchProvider,
};
use navifs_database::SqliteDatabase;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Hybrid search engine combining SQLite FTS5, path/metadata filters, aggressive rank fusion, and feature reranking
pub struct HybridSearchEngine {
    db: Arc<SqliteDatabase>,
    rank_fusion: AggressiveRankFusion,
    embedder: Option<Arc<dyn Embedder>>,
}

impl HybridSearchEngine {
    pub fn new(db: Arc<SqliteDatabase>) -> Self {
        Self {
            db,
            rank_fusion: AggressiveRankFusion::default_aggressive(),
            embedder: None,
        }
    }

    /// Enables the semantic channel: queries are embedded and matched against stored vectors
    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Performs hybrid search combining FTS5 lexical matching, metadata/path filters, aggressive rank fusion,
    /// and multi-feature vector reranking.
    pub async fn search_hybrid(
        &self,
        mut query: HybridSearchQuery,
    ) -> Result<Vec<CandidateResult>> {
        // Embed the query when semantic search is enabled and no vector was supplied
        if query.query_vector.is_none() {
            if let Some(embedder) = &self.embedder {
                query.query_vector = Some(embedder.embed_query(&query.query).await?);
                query.vector_model = Some(embedder.model_id().to_string());
            }
        }
        let fetch_limit = (query.limit * 3).max(30);

        // 1. Channel A: Content FTS5 lexical search
        let content_hits = match self.db.fts_search(&query.query, fetch_limit).await {
            Ok(hits) => hits,
            Err(e) => {
                debug!(
                    "FTS5 content search returned error (query may be empty or syntax): {}",
                    e
                );
                Vec::new()
            }
        };

        // 2. Channel B: Scoped Path and Filename FTS5 search
        let path_hits = match self.db.fts_search_paths(&query.query, fetch_limit).await {
            Ok(hits) => hits,
            Err(e) => {
                debug!("FTS5 path search returned error: {}", e);
                Vec::new()
            }
        };

        // 3. Channel C: Vector Semantic Search Hits (§8.2 & §17)
        let semantic_hits: Vec<SearchHit> = if let (Some(ref q_vec), Some(ref model)) =
            (&query.query_vector, &query.vector_model)
        {
            match self
                .db
                .find_nearest_neighbors(q_vec, model, fetch_limit)
                .await
            {
                Ok(matches) => matches
                    .into_iter()
                    .map(|m| SearchHit {
                        chunk_id: m
                            .chunk_id
                            .map(|c| *c.as_uuid())
                            .unwrap_or_else(uuid::Uuid::nil),
                        file_id: *m.file_id.as_uuid(),
                        score: m.similarity,
                        content: String::new(),
                        path: String::new(),
                        chunk_index: 0,
                    })
                    .collect(),
                Err(e) => {
                    debug!("Vector similarity search error: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // 4. Aggressive Local Rank Fusion across channels (Content + Path + Vector Semantic)
        let fused_candidates = self
            .rank_fusion
            .fuse(&content_hits, &path_hits, &semantic_hits);

        if fused_candidates.is_empty() {
            info!(
                "Hybrid search for '{}' yielded 0 candidates before filtering",
                query.query
            );
            return Ok(Vec::new());
        }

        // 5. Load File Identities and Chunks for Filtering & Reranking
        let mut file_cache: HashMap<FileId, Option<FileIdentity>> = HashMap::new();
        let mut chunk_cache: HashMap<ChunkId, Option<FileChunk>> = HashMap::new();

        for candidate in &fused_candidates {
            if let std::collections::hash_map::Entry::Vacant(e) =
                file_cache.entry(candidate.file_id)
            {
                let file = self.db.get_file(&candidate.file_id).await.unwrap_or(None);
                e.insert(file);
            }
            if let Some(c_id) = candidate.chunk_id {
                if let std::collections::hash_map::Entry::Vacant(e) = chunk_cache.entry(c_id) {
                    let chunk = self.db.get_chunk(&c_id).await.unwrap_or(None);
                    e.insert(chunk);
                }
            }
        }

        // 6. Enforce Path and Metadata Filters
        let filtered_candidates: Vec<_> = fused_candidates
            .into_iter()
            .filter(|c| {
                let file_opt = file_cache.get(&c.file_id).and_then(|f| f.as_ref());

                // Apply Path Filter
                if let Some(ref path_filter) = query.path_filter {
                    let target_path = file_opt.map(|f| f.fingerprint.as_str()).unwrap_or(&c.path);
                    if !path_filter.matches_path(target_path) {
                        return false;
                    }
                }

                // Apply Metadata Filter
                if let Some(ref meta_filter) = query.metadata_filter {
                    match file_opt {
                        Some(file) => {
                            if !meta_filter.matches_file(file) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }

                true
            })
            .collect();

        if filtered_candidates.is_empty() {
            info!(
                "All candidates filtered out by path/metadata constraints for '{}'",
                query.query
            );
            return Ok(Vec::new());
        }

        // 7. Multi-Feature Vector Reranking
        let max_fused = filtered_candidates
            .first()
            .map(|c| c.fused_score)
            .unwrap_or(1.0);

        let reranker = FeatureVectorReranker::new(query.weights.clone());
        let mut scored_candidates = Vec::new();

        for candidate in filtered_candidates {
            let file_opt = file_cache.get(&candidate.file_id).and_then(|f| f.as_ref());
            let chunk_opt = candidate
                .chunk_id
                .and_then(|c_id| chunk_cache.get(&c_id).and_then(|c| c.as_ref()));

            let features =
                reranker.rerank_candidate(&query.query, &candidate, max_fused, file_opt, chunk_opt);

            // 8. Build Evidence Lookup with line/page bounds
            let evidence = EvidenceBuilder::resolve_evidence(
                self.db.as_ref(),
                &candidate,
                file_opt,
                chunk_opt,
            )
            .await;

            let filename = file_opt
                .map(|f| f.fingerprint.filename().to_string())
                .unwrap_or_else(|| candidate.filename.clone());

            let path = file_opt
                .map(|f| f.fingerprint.as_str().to_string())
                .unwrap_or_else(|| candidate.path.clone());

            let mime_type = file_opt
                .map(|f| f.mime_type.to_string())
                .unwrap_or_else(|| "text/plain".to_string());

            scored_candidates.push(CandidateResult {
                file_id: candidate.file_id,
                chunk_id: candidate.chunk_id,
                filename,
                path,
                mime_type,
                score: features.composite_score,
                features,
                evidence,
            });
        }

        // Sort descending by composite score
        scored_candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Retain top K
        scored_candidates.truncate(query.limit);

        info!(
            "Hybrid retrieval complete for '{}': returned {} candidates with evidence lookups",
            query.query,
            scored_candidates.len()
        );

        Ok(scored_candidates)
    }
}

#[async_trait]
impl SearchProvider for HybridSearchEngine {
    async fn index_chunks(&self, chunks: &[FileChunk]) -> Result<()> {
        self.db.save_chunks(chunks).await
    }

    async fn remove_file_from_index(&self, file_id: &FileId) -> Result<()> {
        self.db.delete_chunks_for_file(file_id).await
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let hybrid_query = HybridSearchQuery::new(query).with_limit(limit);
        let candidates = self.search_hybrid(hybrid_query).await?;

        Ok(candidates
            .into_iter()
            .enumerate()
            .map(|(idx, c)| SearchHit {
                chunk_id: c.chunk_id.map(|id| *id.as_uuid()).unwrap_or_default(),
                file_id: *c.file_id.as_uuid(),
                score: c.score,
                content: c.evidence.snippet,
                path: c.path,
                chunk_index: idx as u32,
            })
            .collect())
    }
}
