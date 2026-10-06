//! Local sentence embeddings for NaviFS.
//!
//! Runs BAAI/bge-small-en-v1.5 (384 dimensions) on the pure-Rust `candle` runtime, so the
//! `navifs` binary stays a single self-contained file. The model is downloaded once to the
//! data directory, verified against pinned SHA-256 digests, and then used fully offline.

mod model_store;

pub use model_store::{ensure_model, ModelPaths, MODEL_ID, MODEL_REVISION};

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use navifs_core::{Embedder, NaviError, Result};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// Maximum tokens fed to the model (BERT position limit)
const MAX_TOKENS: usize = 512;

/// Documents are embedded in batches of this size to bound peak memory and maximize throughput
#[cfg(feature = "cuda")]
const BATCH_SIZE: usize = 16;

#[cfg(not(feature = "cuda"))]
const BATCH_SIZE: usize = 32;

/// Instruction BGE models expect in front of search queries (not in front of documents)
const QUERY_INSTRUCTION: &str = "Represent this sentence for searching relevant passages: ";

fn embed_err(context: &str, e: impl std::fmt::Display) -> NaviError {
    NaviError::Embedding(format!("{context}: {e}"))
}

struct Inner {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Inner {
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| embed_err("tokenization failed", e))?;

        let batch = encodings.len();
        let seq_len = encodings.first().map(|e| e.get_ids().len()).unwrap_or(0);
        if seq_len == 0 {
            return Err(NaviError::Embedding("tokenizer produced no tokens".into()));
        }

        let mut ids = Vec::with_capacity(batch * seq_len);
        let mut mask = Vec::with_capacity(batch * seq_len);
        for encoding in &encodings {
            ids.extend_from_slice(encoding.get_ids());
            mask.extend_from_slice(encoding.get_attention_mask());
        }

        let run = || -> candle_core::Result<Vec<Vec<f32>>> {
            let input_ids = Tensor::from_vec(ids, (batch, seq_len), &self.device)?;
            let attention_mask = Tensor::from_vec(mask, (batch, seq_len), &self.device)?;
            let token_type_ids = input_ids.zeros_like()?;

            let hidden = self
                .model
                .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

            // BGE uses the [CLS] token representation, L2-normalised
            let cls = hidden.narrow(1, 0, 1)?.squeeze(1)?;
            let norm = cls.sqr()?.sum_keepdim(1)?.sqrt()?;
            cls.broadcast_div(&norm)?
                .to_dtype(DType::F32)?
                .to_vec2::<f32>()
        };

        match run() {
            Ok(res) => Ok(res),
            Err(e) => {
                if texts.len() > 1 {
                    let mid = texts.len() / 2;
                    let mut first = self.embed_batch(&texts[..mid])?;
                    let second = self.embed_batch(&texts[mid..])?;
                    first.extend(second);
                    Ok(first)
                } else {
                    Err(embed_err("inference failed", e))
                }
            }
        }
    }
}

/// BGE-small embedder running on the CPU through candle
#[derive(Clone)]
pub struct CandleEmbedder {
    inner: Arc<Inner>,
}

impl CandleEmbedder {
    /// Embedding dimensionality of bge-small-en-v1.5
    pub const DIMENSIONS: usize = 384;

    /// Loads the embedder, downloading and verifying the model into `models_dir` on first use
    pub async fn load(models_dir: &Path) -> Result<Self> {
        let models_dir = models_dir.to_path_buf();
        tokio::task::spawn_blocking(move || Self::load_blocking(&models_dir))
            .await
            .map_err(|e| NaviError::Internal(e.to_string()))?
    }

    fn load_blocking(models_dir: &Path) -> Result<Self> {
        let paths = ensure_model(models_dir)?;

        let config_text = std::fs::read_to_string(&paths.config).map_err(NaviError::Io)?;
        let config: Config =
            serde_json::from_str(&config_text).map_err(|e| embed_err("invalid model config", e))?;

        let mut tokenizer = Tokenizer::from_file(&paths.tokenizer)
            .map_err(|e| embed_err("failed to load tokenizer", e))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| embed_err("invalid truncation settings", e))?;
        tokenizer.with_padding(Some(PaddingParams::default()));

        #[cfg(feature = "cuda")]
        let device = match Device::new_cuda(0) {
            Ok(d) => {
                tracing::info!("NaviFS Embedder: accelerated on NVIDIA GPU (CUDA device 0)");
                eprintln!("⚡ NaviFS Embedder: accelerated on NVIDIA GPU (CUDA device 0)");
                d
            }
            Err(e) => {
                tracing::warn!("Failed to initialize CUDA device 0: {e}, falling back to CPU");
                eprintln!("⚠️  CUDA initialization failed ({e}), falling back to CPU");
                Device::Cpu
            }
        };

        #[cfg(not(feature = "cuda"))]
        let device = Device::Cpu;

        // SAFETY: the weights file is verified by SHA-256 before it is moved into place and
        // is never modified afterwards by NaviFS.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[&paths.weights], DTYPE, &device) }
            .map_err(|e| embed_err("failed to map model weights", e))?;
        let model =
            BertModel::load(vb, &config).map_err(|e| embed_err("failed to build model", e))?;

        Ok(Self {
            inner: Arc::new(Inner {
                model,
                tokenizer,
                device,
            }),
        })
    }

    async fn run_batches(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let mut vectors = Vec::with_capacity(texts.len());
            for batch in texts.chunks(BATCH_SIZE) {
                vectors.extend(inner.embed_batch(batch)?);
            }
            Ok(vectors)
        })
        .await
        .map_err(|e| NaviError::Internal(e.to_string()))?
    }
}

#[async_trait]
impl Embedder for CandleEmbedder {
    fn model_id(&self) -> &str {
        MODEL_ID
    }

    fn dimensions(&self) -> usize {
        Self::DIMENSIONS
    }

    async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.run_batches(texts.to_vec()).await
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let prompt = format!("{QUERY_INSTRUCTION}{text}");
        self.run_batches(vec![prompt])
            .await?
            .pop()
            .ok_or_else(|| NaviError::Embedding("no embedding returned for query".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    /// Downloads the real model (~130 MB) on first run; honours NAVIFS_DATA_DIR for the cache.
    /// Run with: cargo test -p navifs-embed -- --ignored
    #[tokio::test]
    #[ignore = "downloads the real embedding model"]
    async fn real_model_ranks_semantically_related_text_higher() {
        let config = navifs_core::EngineConfig::discover(None).unwrap();
        let embedder = CandleEmbedder::load(&config.models_dir()).await.unwrap();
        assert_eq!(embedder.dimensions(), CandleEmbedder::DIMENSIONS);

        let docs = vec![
            "fn read_config(path: &Path) -> Result<Config> { parse the settings file from disk }"
                .to_string(),
            "The recipe calls for two cups of flour and a teaspoon of baking soda.".to_string(),
        ];
        let vectors = embedder.embed_documents(&docs).await.unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0].len(), CandleEmbedder::DIMENSIONS);

        let norm: f32 = vectors[0].iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "vectors must be L2-normalised");

        let query = embedder
            .embed_query("load configuration from a file")
            .await
            .unwrap();
        let code_score = cosine(&query, &vectors[0]);
        let recipe_score = cosine(&query, &vectors[1]);
        assert!(
            code_score > recipe_score + 0.05,
            "code {code_score} should outrank recipe {recipe_score}"
        );
    }
}
