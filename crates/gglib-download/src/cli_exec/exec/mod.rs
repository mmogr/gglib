//! Download execution module.
//!
//! This module handles the actual model download execution using the Python helper.
//! It is intentionally kept separate from queue management.

mod progress;
pub mod python_bridge;
mod python_env;
mod python_protocol;

use std::fs;

use anyhow::{Result, anyhow};
use gglib_core::ports::QuantizationResolver;

use super::types::{CliDownloadRequest, CliDownloadResult, CliUpdateRequest, UpdateCheckResult};
use super::utils::model_directory;
use crate::resolver::HfQuantizationResolver;

pub use python_bridge::{FastDownloadRequest, run_fast_download};

/// Execute a download request and return the result.
///
/// This function:
/// 1. Resolves the quantization files from `HuggingFace`
/// 2. Downloads the files using the Python helper
/// 3. Returns the paths for the handler to register
///
/// Note: This does NOT register the model in the database - that's the
/// handler's responsibility using `ctx.app().models().add()`.
pub async fn download(request: CliDownloadRequest) -> Result<CliDownloadResult> {
    let quant = request.quantization.as_ref().ok_or_else(|| {
        anyhow!("Please specify a quantization. Use --list-quants to see available options.")
    })?;

    println!("Downloading {} from HuggingFace Hub...", request.model_id);

    // Get commit SHA
    let api = super::api::create_hf_api(request.token.clone(), &request.models_dir)?;
    let repo = api.repo(hf_hub::Repo::with_revision(
        request.model_id.clone(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));
    let repo_info = repo
        .info()
        .map_err(|e| anyhow!("Failed to get repo info: {e}"))?;
    let commit_sha = repo_info.sha.clone();
    println!("Found repository, commit SHA: {commit_sha}");

    // Resolve files using the HuggingFace resolver
    println!("Looking for {quant} quantization...");
    let client = gglib_hf::DefaultHfClient::new(&gglib_hf::HfClientConfig::default());
    let resolver = HfQuantizationResolver::new(std::sync::Arc::new(client));

    let quantization = gglib_core::download::Quantization::from_filename(quant);
    let resolution = resolver.resolve(&request.model_id, quantization).await
        .map_err(|e| anyhow!(
            "No GGUF file found for quantization '{quant}'. Use --list-quants to see available options. Error: {e}",
        ))?;

    let files: Vec<String> = resolution.files.iter().map(|f| f.path.clone()).collect();
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
    let model_dir = model_directory(&request.models_dir, &request.model_id);
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    // Download files
    let fast_request = FastDownloadRequest {
        repo_id: &request.model_id,
        revision: &commit_sha,
        repo_type: "model",
        destination: &model_dir,
        files: &files,
        token: request.token.as_deref(),
        force: request.force,
        progress: None,
        cancel_token: None,
    };

    run_fast_download(&fast_request).await?;
    println!("⚡ Downloaded via fast helper");

    let primary_path = model_dir.join(&files[0]);
    let all_paths: Vec<_> = files.iter().map(|f| model_dir.join(f)).collect();

    println!(
        "✓ Successfully downloaded {} to {}",
        request.model_id,
        model_dir.display()
    );

    Ok(CliDownloadResult {
        downloaded_paths: all_paths,
        primary_path,
        quantization: quant.clone(),
        repo_id: request.model_id,
        commit_sha,
    })
}

/// Check if a model has an update available.
pub async fn check_update(
    repo_id: &str,
    current_sha: Option<&str>,
    models_dir: &std::path::Path,
) -> Result<UpdateCheckResult> {
    let api = super::api::create_hf_api(None, models_dir)?;
    let repo = api.repo(hf_hub::Repo::with_revision(
        repo_id.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    let repo_info = repo
        .info()
        .map_err(|e| anyhow!("Failed to get repo info: {e}"))?;

    let latest_sha = repo_info.sha;
    let has_update = current_sha.is_none_or(|s| s != latest_sha);

    Ok(UpdateCheckResult {
        has_update,
        current_sha: current_sha.map(String::from),
        latest_sha,
    })
}

/// Update a model to the latest version.
pub async fn update_model(request: CliUpdateRequest) -> Result<CliDownloadResult> {
    // Reuse the download logic with force=true
    let download_request = CliDownloadRequest {
        model_id: request.repo_id,
        quantization: Some(request.quantization),
        models_dir: request.models_dir,
        force: true,
        token: request.token,
    };

    download(download_request).await
}
