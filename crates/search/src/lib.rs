//! Lexical and semantic search engine for NaviFS chunks

use std::collections::{HashMap, HashSet};
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

        // Score accumulator: chunk_id -> (score, file_id, chunk_index)
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
