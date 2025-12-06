//! Model registration workflow.
//!
//! Registers downloaded models in the local database after download completes.
//! This module is independently testable and has no download logic.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::download::domain::config::DownloadResult;
use crate::download::domain::types::Quantization;
use crate::models::Gguf;
use crate::services::AppCore;
use crate::utils::validation;

/// Register a downloaded model in the database.
///
/// Parses GGUF metadata from the downloaded file and creates a database entry.
/// For sharded models, the primary (first shard) path is used for registration.
///
/// # Arguments
///
/// * `core` - AppCore for database access
/// * `result` - The download result containing paths and metadata
///
/// # Returns
///
/// Returns the created `Gguf` model on success.
pub async fn register_model(core: Arc<AppCore>, result: &DownloadResult) -> Result<Gguf> {
    let file_path = result.db_path();
    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid file path"))?;

    // Parse GGUF metadata from the downloaded file
    let gguf_metadata = match validation::validate_and_parse_gguf(file_path_str) {
        Ok(metadata) => {
            println!("✓ Parsed GGUF metadata from downloaded file");
            Some(metadata)
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to parse GGUF metadata, using defaults: {}",
                e
            );
            None
        }
    };

    // Extract param_count_b from metadata, fall back to 0.0
    let param_count_b = gguf_metadata
        .as_ref()
        .and_then(|m| m.param_count_b)
        .unwrap_or(0.0);

    let mut model = Gguf::new(
        result.repo_id.clone(),
        file_path.to_path_buf(),
        param_count_b,
        Utc::now(),
    );

    // Use extracted metadata where available, with fallbacks
    model.quantization = gguf_metadata
        .as_ref()
        .and_then(|m| m.quantization.clone())
        .or_else(|| Some(result.quantization.to_string()));
    model.architecture = gguf_metadata.as_ref().and_then(|m| m.architecture.clone());
    model.context_length = gguf_metadata.as_ref().and_then(|m| m.context_length);
    if let Some(ref meta) = gguf_metadata {
        model.metadata = meta.metadata.clone();
    }
    model.hf_repo_id = Some(result.repo_id.clone());
    model.hf_commit_sha = Some(result.commit_sha.clone());
    model.hf_filename = Some(file_path.file_name().unwrap().to_string_lossy().to_string());
    model.download_date = Some(Utc::now());

    // Auto-detect reasoning and tool calling capabilities from metadata
    if let Some(ref meta) = gguf_metadata {
        let tags = crate::gguf::apply_capability_detection(&meta.metadata);
        model.tags = tags;
    }

    // For sharded models, add a note about the number of shards
    if result.is_sharded {
        let shard_count = result.all_paths.len();
        if let Some(ref mut quant) = model.quantization {
            *quant = format!("{} (sharded: {} parts)", quant, shard_count);
        }
    }

    println!("Adding model to database...");

    core.models().add(&model).await?;

    println!("✓ Successfully added model to database:");
    println!("  Model: {}", model.name);
    println!("  File: {}", model.file_path.display());
    println!("  Parameters: {:.1}B", model.param_count_b);
    println!("  Quantization: {:?}", model.quantization);
    println!("  Architecture: {:?}", model.architecture);
    println!("  HF Repo: {:?}", model.hf_repo_id);

    Ok(model)
}

/// Register a model using raw parameters (for backwards compatibility with CLI).
///
/// Prefer using `register_model` with a `DownloadResult` when possible.
pub async fn register_model_from_path(
    core: Arc<AppCore>,
    repo_id: &str,
    commit_sha: &str,
    file_path: &Path,
    quantization: &str,
) -> Result<()> {
    let result = DownloadResult {
        primary_path: file_path.to_path_buf(),
        all_paths: vec![file_path.to_path_buf()],
        quantization: Quantization::from_filename(quantization),
        repo_id: repo_id.to_string(),
        commit_sha: commit_sha.to_string(),
        is_sharded: false,
        total_bytes: 0,
    };

    register_model(core, &result).await?;
    Ok(())
}
