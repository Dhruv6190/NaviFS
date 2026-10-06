//! Feature vector reranker evaluating lexical score, path similarity, freshness, and extraction quality

use crate::fusion::FusedCandidate;
use crate::types::{RerankerFeatures, RerankerWeights};
use chrono::{DateTime, Utc};
use navifs_core::{FileChunk, FileIdentity};

/// Multi-feature vector reranker that refines fused candidates into an optimal top-K ranking
pub struct FeatureVectorReranker {
    weights: RerankerWeights,
}

impl FeatureVectorReranker {
    pub fn new(weights: RerankerWeights) -> Self {
        Self { weights }
    }

    pub fn default_reranker() -> Self {
        Self::new(RerankerWeights::default())
    }

    /// Computes normalized feature vectors and composite scores for all candidates
    pub fn rerank_candidate(
        &self,
        query: &str,
        candidate: &FusedCandidate,
        max_fused_score: f32,
        file: Option<&FileIdentity>,
        chunk: Option<&FileChunk>,
    ) -> RerankerFeatures {
        let lexical_score = self.compute_lexical_score(candidate.fused_score, max_fused_score);
        let path_similarity =
            self.compute_path_similarity(query, &candidate.path, &candidate.filename);
        let freshness_score = self.compute_freshness(file.map(|f| f.modified_at));
        let extraction_quality = self.compute_extraction_quality(chunk, &candidate.snippet);

        let composite_score = (self.weights.lexical_weight * lexical_score)
            + (self.weights.path_similarity_weight * path_similarity)
            + (self.weights.freshness_weight * freshness_score)
            + (self.weights.quality_weight * extraction_quality);

        RerankerFeatures {
            lexical_score,
            path_similarity,
            freshness_score,
            extraction_quality,
            composite_score,
        }
    }

    /// Normalizes lexical fused score to [0.0, 1.0] range
    fn compute_lexical_score(&self, fused_score: f32, max_fused_score: f32) -> f32 {
        if max_fused_score <= 0.0001 {
            return 0.5;
        }
        (fused_score / max_fused_score).clamp(0.0, 1.0)
    }

    /// Evaluates path and filename similarity to the user query
    fn compute_path_similarity(&self, query: &str, path: &str, filename: &str) -> f32 {
        let clean_query = query.to_lowercase();
        let query_tokens: Vec<&str> = clean_query
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|t| t.len() > 1)
            .collect();

        if query_tokens.is_empty() {
            return 0.5;
        }

        let clean_path = path.to_lowercase().replace('\\', "/");
        let clean_filename = filename.to_lowercase();
        let stem = std::path::Path::new(&clean_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&clean_filename);

        // 1. Exact match bonus on filename stem
        if query_tokens
            .iter()
            .any(|&token| token == stem || stem.contains(token))
        {
            let depth = clean_path.chars().filter(|&c| c == '/').count();
            let depth_factor = 1.0 / (1.0 + 0.05 * depth as f32);
            return (0.95 * depth_factor).clamp(0.6, 1.0);
        }

        // 2. Token overlap across path segments
        let mut matches = 0usize;
        for token in &query_tokens {
            if clean_path.contains(token) {
                matches += 1;
            }
        }

        let overlap_ratio = matches as f32 / query_tokens.len() as f32;
        let depth = clean_path.chars().filter(|&c| c == '/').count();
        let depth_factor = 1.0 / (1.0 + 0.05 * depth as f32);

        (overlap_ratio * depth_factor).clamp(0.0, 1.0)
    }

    /// Computes recency / freshness decay score via exponential curve (30-day scale)
    fn compute_freshness(&self, modified_at: Option<DateTime<Utc>>) -> f32 {
        let mod_time = match modified_at {
            Some(t) => t,
            None => return 0.70, // Safe neutral freshness if unknown
        };

        let now = Utc::now();
        let elapsed_seconds = now.signed_duration_since(mod_time).num_seconds().max(0);
        let elapsed_days = elapsed_seconds as f32 / 86400.0;

        // Exponential decay: e^(-days / 30.0)
        // Day 0: 1.0, Day 7: ~0.79, Day 30: ~0.37, Day 90: ~0.05
        (-elapsed_days / 30.0).exp().clamp(0.05, 1.0)
    }

    /// Evaluates structural quality of the chunk and its index locator bounds
    fn compute_extraction_quality(&self, chunk: Option<&FileChunk>, fallback_snippet: &str) -> f32 {
        let mut quality = 0.0f32;

        match chunk {
            Some(c) => {
                // 1. Line range presence
                if c.line_range.is_some() {
                    quality += 0.25;
                }

                // 2. Page range or structural section presence
                if c.page_range.is_some()
                    || matches!(c.chunk_type, navifs_core::ChunkType::MarkdownSection { .. })
                {
                    quality += 0.25;
                }

                // 3. Optimal length sizing (penalize tiny stubs < 40 chars, reward rich informative chunks)
                let len = c.content.len();
                if (150..=2500).contains(&len) {
                    quality += 0.30;
                } else if (40..150).contains(&len) {
                    quality += 0.20;
                } else if len > 2500 {
                    quality += 0.25;
                } else {
                    quality += 0.05;
                }

                // 4. Content density and readable character ratio
                let alphanumeric_count = c
                    .content
                    .chars()
                    .filter(|ch| ch.is_alphanumeric() || ch.is_whitespace())
                    .count();
                let ratio = alphanumeric_count as f32 / len.max(1) as f32;
                if ratio > 0.8 {
                    quality += 0.20;
                } else {
                    quality += 0.10;
                }
            }
            None => {
                // Evaluate based on snippet if chunk record is not preloaded
                if fallback_snippet.len() > 60 {
                    quality += 0.60;
                } else {
                    quality += 0.30;
                }
            }
        }

        quality.clamp(0.1, 1.0)
    }
}
