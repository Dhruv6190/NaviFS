//! Aggressive local rank fusion combining multi-channel lexical, path, and full-text matches

use std::collections::HashMap;
use navifs_core::{ChunkId, FileId, SearchHit};
use navifs_database::FtsSearchResult;

/// Aggressive rank fusion configuration
#[derive(Debug, Clone)]
pub struct RankFusionConfig {
    /// RRF smoothing constant (lower values like 20.0 create aggressive rank separation)
    pub k: f32,
    /// Channel weight for content FTS5
    pub content_weight: f32,
    /// Channel weight for path/filename FTS5
    pub path_weight: f32,
    /// Channel weight for inverted index lexical matches
    pub inverted_weight: f32,
    /// Multiplier bonus per additional channel matching the same entity
    pub multi_channel_bonus: f32,
}

impl Default for RankFusionConfig {
    fn default() -> Self {
        Self {
            k: 20.0,
            content_weight: 1.0,
            path_weight: 1.8,
            inverted_weight: 0.8,
            multi_channel_bonus: 0.35,
        }
    }
}

/// Consolidated intermediate candidate produced during rank fusion
#[derive(Debug, Clone)]
pub struct FusedCandidate {
    pub file_id: FileId,
    pub chunk_id: Option<ChunkId>,
    pub filename: String,
    pub path: String,
    pub snippet: String,
    pub fused_score: f32,
    pub channel_hits: usize,
    pub raw_bm25_score: f32,
}

pub struct AggressiveRankFusion {
    config: RankFusionConfig,
}

impl AggressiveRankFusion {
    pub fn new(config: RankFusionConfig) -> Self {
        Self { config }
    }

    pub fn default_aggressive() -> Self {
        Self::new(RankFusionConfig::default())
    }

    /// Fuses multiple retrieval channels:
    /// - `content_hits`: FTS5 content search
    /// - `path_hits`: FTS5 path & filename scoped search
    /// - `lexical_hits`: Inverted-index lexical hits
    pub fn fuse(
        &self,
        content_hits: &[FtsSearchResult],
        path_hits: &[FtsSearchResult],
        lexical_hits: &[SearchHit],
    ) -> Vec<FusedCandidate> {
        // Map from entity key (file_id, chunk_id) -> accumulator
        let mut accumulators: HashMap<(FileId, Option<ChunkId>), FusedCandidate> = HashMap::new();

        // 1. Channel 1: Content FTS5
        for (rank, hit) in content_hits.iter().enumerate() {
            let key = (hit.file_id, hit.chunk_id);
            let rrf_contribution = self.config.content_weight / (self.config.k + (rank + 1) as f32);

            let entry = accumulators.entry(key).or_insert_with(|| FusedCandidate {
                file_id: hit.file_id,
                chunk_id: hit.chunk_id,
                filename: hit.filename.clone(),
                path: hit.path.clone(),
                snippet: hit.snippet.clone(),
                fused_score: 0.0,
                channel_hits: 0,
                raw_bm25_score: hit.score,
            });

            entry.fused_score += rrf_contribution;
            entry.channel_hits += 1;
            entry.raw_bm25_score = entry.raw_bm25_score.max(hit.score);
            if entry.snippet.is_empty() {
                entry.snippet = hit.snippet.clone();
            }
        }

        // 2. Channel 2: Path & Filename FTS5
        for (rank, hit) in path_hits.iter().enumerate() {
            let key = (hit.file_id, hit.chunk_id);
            let rrf_contribution = self.config.path_weight / (self.config.k + (rank + 1) as f32);

            let entry = accumulators.entry(key).or_insert_with(|| FusedCandidate {
                file_id: hit.file_id,
                chunk_id: hit.chunk_id,
                filename: hit.filename.clone(),
                path: hit.path.clone(),
                snippet: hit.snippet.clone(),
                fused_score: 0.0,
                channel_hits: 0,
                raw_bm25_score: hit.score,
            });

            entry.fused_score += rrf_contribution;
            entry.channel_hits += 1;
            entry.raw_bm25_score = entry.raw_bm25_score.max(hit.score);
            if entry.snippet.is_empty() {
                entry.snippet = hit.snippet.clone();
            }
        }

        // 3. Channel 3: Inverted Index Lexical Hits
        for (rank, hit) in lexical_hits.iter().enumerate() {
            let file_id = FileId::from_uuid(hit.file_id);
            let chunk_id = Some(ChunkId::from_uuid(hit.chunk_id));
            let key = (file_id, chunk_id);
            let rrf_contribution = self.config.inverted_weight / (self.config.k + (rank + 1) as f32);

            let entry = accumulators.entry(key).or_insert_with(|| FusedCandidate {
                file_id,
                chunk_id,
                filename: String::new(),
                path: hit.path.clone(),
                snippet: hit.content.chars().take(150).collect(),
                fused_score: 0.0,
                channel_hits: 0,
                raw_bm25_score: hit.score,
            });

            entry.fused_score += rrf_contribution;
            entry.channel_hits += 1;
            entry.raw_bm25_score = entry.raw_bm25_score.max(hit.score);
        }

        // Apply aggressive cross-channel mutual reinforcement bonus
        let mut candidates: Vec<FusedCandidate> = accumulators
            .into_values()
            .map(|mut c| {
                if c.channel_hits > 1 {
                    let bonus_multiplier = 1.0 + (c.channel_hits - 1) as f32 * self.config.multi_channel_bonus;
                    c.fused_score *= bonus_multiplier;
                }
                c
            })
            .collect();

        // Sort by fused score descending
        candidates.sort_by(|a, b| {
            b.fused_score
                .partial_cmp(&a.fused_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
    }
}
