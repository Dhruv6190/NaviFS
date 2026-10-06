//! Download, pinning and integrity verification of the embedding model files.
//!
//! Model files are fetched once from a fixed Hugging Face commit and verified against
//! SHA-256 digests compiled into the binary, so a tampered or truncated download is rejected.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use navifs_core::{NaviError, Result};
use sha2::{Digest, Sha256};

/// Identifier stored alongside every vector produced with this model
pub const MODEL_ID: &str = "BAAI/bge-small-en-v1.5";

/// Immutable Hugging Face commit the model files are pinned to
pub const MODEL_REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";

/// A file belonging to the model, with its expected size and SHA-256 digest
pub struct PinnedFile {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const CONFIG_FILE: PinnedFile = PinnedFile {
    name: "config.json",
    size: 743,
    sha256: "094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750",
};

pub const TOKENIZER_FILE: PinnedFile = PinnedFile {
    name: "tokenizer.json",
    size: 711_396,
    sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
};

pub const WEIGHTS_FILE: PinnedFile = PinnedFile {
    name: "model.safetensors",
    size: 133_466_304,
    sha256: "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad",
};

/// Local paths of the verified model files
#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub config: PathBuf,
    pub tokenizer: PathBuf,
    pub weights: PathBuf,
}

/// Ensures all model files exist under `models_dir`, downloading and verifying any that are
/// missing. Blocking; call from a blocking context.
pub fn ensure_model(models_dir: &Path) -> Result<ModelPaths> {
    let dir = models_dir
        .join(MODEL_ID.replace('/', "--"))
        .join(MODEL_REVISION);
    fs::create_dir_all(&dir).map_err(NaviError::Io)?;

    Ok(ModelPaths {
        config: ensure_file(&dir, &CONFIG_FILE)?,
        tokenizer: ensure_file(&dir, &TOKENIZER_FILE)?,
        weights: ensure_file(&dir, &WEIGHTS_FILE)?,
    })
}

fn ensure_file(dir: &Path, file: &PinnedFile) -> Result<PathBuf> {
    let target = dir.join(file.name);

    // A file only ever appears at its final path after full hash verification, so a
    // matching size is enough to trust it on later starts.
    if let Ok(meta) = fs::metadata(&target) {
        if meta.len() == file.size {
            return Ok(target);
        }
        fs::remove_file(&target).map_err(NaviError::Io)?;
    }

    download(file, &target)?;
    Ok(target)
}

fn download(file: &PinnedFile, target: &Path) -> Result<()> {
    let url = format!(
        "https://huggingface.co/{MODEL_ID}/resolve/{MODEL_REVISION}/{}",
        file.name
    );
    // stderr only: stdout belongs to the MCP JSON-RPC stream
    eprintln!(
        "navifs: downloading {} ({:.1} MB) from {url}",
        file.name,
        file.size as f64 / 1_048_576.0
    );

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(1800)))
        .build()
        .into();
    let response = agent
        .get(&url)
        .call()
        .map_err(|e| NaviError::Embedding(format!("failed to download {url}: {e}")))?;
    let mut reader = response.into_body().into_reader();

    let partial = target.with_extension("part");
    let digest = {
        let mut out = File::create(&partial).map_err(NaviError::Io)?;
        let digest = copy_with_sha256(&mut reader, &mut out)?;
        out.flush().map_err(NaviError::Io)?;
        digest
    };

    if let Err(e) = verify_digest(&digest, file) {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    fs::rename(&partial, target).map_err(NaviError::Io)?;
    Ok(())
}

fn copy_with_sha256(reader: &mut impl Read, writer: &mut impl Write) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| NaviError::Embedding(format!("download interrupted: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n]).map_err(NaviError::Io)?;
    }
    Ok(hex::encode(hasher.finalize()))
}

fn verify_digest(actual: &str, file: &PinnedFile) -> Result<()> {
    if actual == file.sha256 {
        Ok(())
    } else {
        Err(NaviError::Embedding(format!(
            "integrity check failed for {}: expected sha256 {}, got {}",
            file.name, file.sha256, actual
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_of_known_input_matches_sha256() {
        let mut out = Vec::new();
        let digest = copy_with_sha256(&mut &b"abc"[..], &mut out).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(out, b"abc");
    }

    #[test]
    fn mismatching_digest_is_rejected() {
        let err = verify_digest("deadbeef", &CONFIG_FILE).unwrap_err();
        assert!(err.to_string().contains("integrity check failed"));
    }

    #[test]
    fn existing_file_with_correct_size_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE.name);
        fs::write(&path, vec![b'x'; CONFIG_FILE.size as usize]).unwrap();
        assert_eq!(ensure_file(dir.path(), &CONFIG_FILE).unwrap(), path);
    }
}
