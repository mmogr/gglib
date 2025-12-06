//! Model download orchestration.
//!
//! This module provides the main entry point for downloading models from HuggingFace.
//! It delegates to the resolver for file discovery and to the python bridge for
//! actual downloads.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use hf_hub::api::sync::Api;
use tokio_util::sync::CancellationToken;

use super::file_ops::ProgressCallback;
use super::python_bridge::{FastDownloadRequest, run_fast_download};
use super::utils::sanitize_model_name;
use crate::download::huggingface::QuantizationFileResolver;
use crate::download::workflows::register_model_from_path;
use crate::services::AppCore;
use crate::services::core::PidStorage;
use std::sync::Arc;

// Re-export update handlers from the new location
pub use crate::download::update::{check_model_update, handle_check_updates, handle_update_model};

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for downloading sharded files (used by file_ops).
pub struct DownloadConfig<'a> {
    pub model_id: &'a str,
    pub commit_sha: &'a str,
    pub models_dir: &'a Path,
    pub force: bool,
    pub add_to_db: bool,
    pub quantization: &'a str,
}

/// Session-specific options shared across download helpers.
#[derive(Default)]
pub struct SessionOptions<'a> {
    pub auth_token: Option<String>,
    pub progress_callback: Option<&'a ProgressCallback>,
    pub cancel_token: Option<CancellationToken>,
    pub pid_storage: Option<PidStorage>,
    pub pid_key: Option<String>,
}

impl SessionOptions<'_> {
    pub fn token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }
}

/// Consolidated context passed through download operations.
pub struct DownloadContext<'a> {
    pub model_id: &'a str,
    pub quantization: Option<&'a str>,
    pub models_dir: &'a Path,
    pub force: bool,
    pub add_to_db: bool,
    pub session: SessionOptions<'a>,
    pub first_shard_path: Option<PathBuf>,
    /// AppCore for database operations (optional for backwards compatibility)
    pub core: Option<Arc<AppCore>>,
}

// ============================================================================
// Download Orchestration
// ============================================================================

/// Download a specific model with optional quantization filter.
///
/// This is the main entry point for CLI downloads. It:
/// 1. Resolves files using the HuggingFace resolver
/// 2. Downloads files via the Python helper
/// 3. Registers the model in the database if requested
pub async fn download_model(api: &Api, context: DownloadContext<'_>) -> Result<()> {
    let quant = context.quantization.ok_or_else(|| {
        anyhow!("Please specify a quantization. Use --list-quants to see available options.")
    })?;

    println!("Downloading {} from HuggingFace Hub...", context.model_id);

    // Get repository info for commit SHA
    let repo = api.repo(hf_hub::Repo::with_revision(
        context.model_id.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));
    let repo_info = repo
        .info()
        .map_err(|e| anyhow!("Failed to get repo info: {}", e))?;
    let commit_sha = repo_info.sha.clone();
    println!("Found repository, commit SHA: {}", commit_sha);

    // Resolve files using the HuggingFace resolver
    println!("Looking for {} quantization...", quant);
    let resolver = QuantizationFileResolver::new();
    let resolution = resolver.resolve(context.model_id, quant).await.map_err(|e| {
        anyhow!(
            "No GGUF file found for quantization '{}'. Use --list-quants to see available options. Error: {}",
            quant,
            e
        )
    })?;

    let files = resolution.filenames();
    if resolution.is_sharded {
        println!(
            "✓ Found {} sharded files for quantization {}",
            files.len(),
            quant
        );
    } else {
        println!("✓ Found file: {}", files[0]);
    }

    // Prepare destination directory
    let model_dir = context
        .models_dir
        .join(sanitize_model_name(context.model_id));
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    // Download files
    let fast_request = FastDownloadRequest {
        repo_id: context.model_id,
        revision: &commit_sha,
        repo_type: "model",
        destination: &model_dir,
        files: &files,
        token: context.session.token(),
        force: context.force,
        progress: context.session.progress_callback,
        cancel_token: context.session.cancel_token.clone(),
        pid_storage: context.session.pid_storage.clone(),
        pid_key: context.session.pid_key.clone(),
    };

    run_fast_download(&fast_request).await?;
    println!("⚡ Downloaded via fast helper");

    // Register in database if requested
    if context.add_to_db {
        if let Some(core) = &context.core {
            let primary_path = model_dir.join(&files[0]);
            register_model_from_path(
                Arc::clone(core),
                context.model_id,
                &commit_sha,
                &primary_path,
                quant,
            )
            .await?;
        } else {
            return Err(anyhow!("AppCore required for database registration"));
        }
    }

    println!(
        "✓ Successfully downloaded {} to {}",
        context.model_id,
        model_dir.display()
    );
    Ok(())
}

/// Add downloaded model to database (backwards compatibility shim).
///
/// Prefer using `crate::download::workflows::register_model` directly.
pub async fn add_to_database(
    core: Arc<AppCore>,
    model_id: &str,
    commit_sha: &str,
    file_path: &Path,
    quantization: &str,
) -> Result<()> {
    register_model_from_path(core, model_id, commit_sha, file_path, quantization).await
}
