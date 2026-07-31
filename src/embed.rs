//! Local embedding model for exemplar similarity (`transcripts similar`).
//!
//! The model is `all-MiniLM-L6-v2` (BERT architecture) running in-process via
//! candle on CPU. Weights are fetched once, explicitly, by `spotter embed
//! init` into a cache dir (see [`paths::model_dir`]); inference itself is
//! fully local and offline. There are no cloud calls anywhere in this path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use crate::db::{self, ToolCallRun};
use crate::paths;

/// Model id: cache directory name and the `run_embeddings.model` key.
pub const MODEL_ID: &str = "all-MiniLM-L6-v2";

const MODEL_REPO: &str = "sentence-transformers/all-MiniLM-L6-v2";
const CONFIG_FILE: &str = "config.json";
const TOKENIZER_FILE: &str = "tokenizer.json";
const WEIGHTS_FILE: &str = "model.safetensors";
const MODEL_FILES: [&str; 3] = [CONFIG_FILE, TOKENIZER_FILE, WEIGHTS_FILE];

/// Cap on the text embedded per run; `MiniLM` truncates at 512 tokens anyway.
const MAX_DOCUMENT_CHARS: usize = 1_000;
const MAX_TOKENS: usize = 512;

/// Paths of the three files the model consists of.
#[derive(Debug, Clone)]
pub struct ModelFiles {
    /// BERT configuration.
    pub config: PathBuf,
    /// Tokenizer vocabulary.
    pub tokenizer: PathBuf,
    /// Safetensors weights.
    pub weights: PathBuf,
}

/// Compose the cache paths for a model directory.
pub fn model_files(dir: &Path) -> ModelFiles {
    ModelFiles {
        config: dir.join(CONFIG_FILE),
        tokenizer: dir.join(TOKENIZER_FILE),
        weights: dir.join(WEIGHTS_FILE),
    }
}

/// Return the model files, failing fast with the remedy when absent.
pub fn ensure_model(dir: &Path) -> Result<ModelFiles> {
    let files = model_files(dir);
    let missing = MODEL_FILES
        .iter()
        .filter(|name| !dir.join(name).exists())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!(
            "embedding model not found at {} (missing: {}); run `spotter embed init` to download it",
            dir.display(),
            missing
                .iter()
                .map(|name| (*(*name)).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(files)
}

/// Download the model files into `dir` via `curl` (fail-fast, `.part` files
/// renamed into place so a partial download never looks complete).
///
/// This is the only network-touching code in spotter, and it runs only on an
/// explicit user command. It shells out to `curl` so the binary carries no
/// HTTP client (see `scripts/check-local-only.py`).
pub fn download_model(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)
        .with_context(|| format!("failed to create model dir {}", dir.display()))?;
    for name in MODEL_FILES {
        let target = dir.join(name);
        if target.exists() {
            continue;
        }
        let url = format!("https://huggingface.co/{MODEL_REPO}/resolve/main/{name}");
        let partial = dir.join(format!("{name}.part"));
        let status = Command::new("curl")
            .arg("-fSL")
            .arg("--retry")
            .arg("3")
            .arg("-o")
            .arg(&partial)
            .arg(&url)
            .status()
            .context("failed to run curl; install curl or place the model files manually")?;
        if !status.success() {
            let _ = fs::remove_file(&partial);
            anyhow::bail!("download failed for {url} (curl exit {status})");
        }
        fs::rename(&partial, &target)
            .with_context(|| format!("failed to move {} into place", target.display()))?;
    }
    Ok(())
}

/// The in-process `MiniLM` embedder.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Embedder {
    /// Load config, tokenizer, and weights from disk (fully offline).
    pub fn load(files: &ModelFiles) -> Result<Self> {
        let device = Device::Cpu;
        let config: BertConfig = serde_json::from_str(
            &fs::read_to_string(&files.config)
                .with_context(|| format!("failed to read {}", files.config.display()))?,
        )
        .with_context(|| format!("failed to parse {}", files.config.display()))?;
        let tensors = candle_core::safetensors::load(&files.weights, &device)
            .with_context(|| format!("failed to load {}", files.weights.display()))?;
        let model = BertModel::load(
            VarBuilder::from_tensors(tensors, DType::F32, &device),
            &config,
        )
        .context("failed to build the BERT model from weights")?;
        let mut tokenizer = Tokenizer::from_file(&files.tokenizer)
            .map_err(|error| anyhow::anyhow!("failed to load tokenizer: {error}"))?;
        let _ = tokenizer.with_truncation(Some(TruncationParams {
            max_length: MAX_TOKENS,
            ..Default::default()
        }));
        let _ = tokenizer.with_padding(Some(PaddingParams::default()));
        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    /// Embed one text into an L2-normalized vector (dot product = cosine).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|error| anyhow::anyhow!("tokenization failed: {error}"))?;
        let seq_len = encoding.len();
        let input_ids = Tensor::from_vec(encoding.get_ids().to_vec(), (1, seq_len), &self.device)?;
        let token_type_ids =
            Tensor::from_vec(encoding.get_type_ids().to_vec(), (1, seq_len), &self.device)?;
        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids)
            .context("BERT forward pass failed")?;
        let pooled = hidden.squeeze(0)?.mean(0)?;
        let norm = pooled.sqr()?.sum_all()?.sqrt()?.to_vec0::<f32>()?;
        let mut vector = pooled.to_vec1::<f32>()?;
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        Ok(vector)
    }
}

/// The compact text document embedded for one run: tool name, command or
/// input summary, and error content — the vocabulary a failure shares with
/// its look-alikes.
pub fn run_document(run: &ToolCallRun) -> String {
    let mut document = run.tool_name.clone();
    if let Some(command) = &run.command {
        document.push(' ');
        document.push_str(command);
    } else if let Some(summary) = &run.input_summary {
        document.push(' ');
        document.push_str(summary);
    }
    if let Some(error) = &run.error_content {
        document.push_str(" error: ");
        document.push_str(error);
    }
    db::truncate_chars(&document, MAX_DOCUMENT_CHARS)
}

/// Cosine similarity of L2-normalized vectors (a dot product).
pub fn cosine(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum()
}

/// Look up a cached embedding, computing and caching it on first use.
pub fn embedding_for(
    conn: &rusqlite::Connection,
    embedder: &Embedder,
    run: &ToolCallRun,
) -> Result<Vec<f32>> {
    if let Some(embedding) = db::get_embedding(conn, &run.session_id, &run.tool_use_id, MODEL_ID)? {
        return Ok(embedding);
    }
    let embedding = embedder.embed(&run_document(run))?;
    db::upsert_embedding(
        conn,
        &run.session_id,
        &run.tool_use_id,
        MODEL_ID,
        &embedding,
    )?;
    Ok(embedding)
}

/// Resolve the model directory and fail fast when the model is absent.
pub fn require_model(override_dir: Option<PathBuf>) -> Result<ModelFiles> {
    let dir = paths::model_dir(override_dir)?;
    ensure_model(&dir)
}
