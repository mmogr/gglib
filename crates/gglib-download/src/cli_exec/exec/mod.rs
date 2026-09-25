#![doc = include_str!("README.md")]
mod progress;
pub(crate) mod python_bridge;
pub(crate) mod python_env;
mod python_protocol;
mod xet_poller;

use std::fs;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use gglib_core::ports::{HfClientPort, QuantizationResolver};

use super::types::{CliDownloadRequest, CliDownloadResult, CliUpdateRequest, UpdateCheckResult};
use super::utils::model_directory;
use crate::executor::{DownloadPlan, download_files};
use crate::resolver::HfQuantizationResolver;

/// Execute a download request and return the result.
///
/// Used internally by [`update_model`] for the force-redownload path.
/// Interactive CLI downloads now route through
/// [`DownloadManagerPort::queue_smart`](gglib_core::ports::DownloadManagerPort::queue_smart)
/// instead of calling this function directly.
pub(super) async fn download(request: CliDownloadRequest) -> Result<CliDownloadResult> {
    let quant = request.quantization.as_ref().ok_or_else(|| {
        anyhow!("Please specify a quantization. Use --list-quants to see available options.")
    })?;

    gglib_core::telemetry::console_println(&format!(
        "Downloading {} from HuggingFace Hub...",
        request.model_id
    ));

    let hub: Arc<dyn HfClientPort> = Arc::new(super::api::hub_client(request.token.clone()));
    let commit_sha = hub
        .get_commit_sha(&request.model_id)
        .await
        .map_err(|e| anyhow!("Failed to get repo info: {e}"))?;
    gglib_core::telemetry::console_println(&format!("Found repository, commit SHA: {commit_sha}"));

    // Resolve files using the HuggingFace resolver
    gglib_core::telemetry::console_println(&format!("Looking for {quant} quantization..."));
    let resolver = HfQuantizationResolver::new(hub);

    let quantization = gglib_core::download::Quantization::from_filename(quant);
    let resolution = resolver.resolve(&request.model_id, quantization).await
        .map_err(|e| anyhow!(
            "No GGUF file found for quantization '{quant}'. Use --list-quants to see available options. Error: {e}",
        ))?;

    let files: Vec<String> = resolution.files.iter().map(|f| f.path.clone()).collect();
    if resolution.is_sharded {
        gglib_core::telemetry::console_println(&format!(
            "✓ Found {} sharded files for quantization {}",
            files.len(),
            quant
        ));
    } else {
        gglib_core::telemetry::console_println(&format!("✓ Found file: {}", files[0]));
    }

    // Prepare destination directory
    let model_dir = model_directory(&request.models_dir, &request.model_id);
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    // Download files. `expected_total` is the summed size of everything being
    // fetched, which is only attributable to a single file when there is one.
    let expected_total = (files.len() == 1)
        .then(|| resolution.files.first().and_then(|f| f.size))
        .flatten();

    let plan = DownloadPlan {
        repo_id: &request.model_id,
        revision: &commit_sha,
        destination: &model_dir,
        files: &files,
        token: request.token.as_deref(),
        force: request.force,
        progress: None,
        notice: None,
        expected_total,
        cancel: None,
    };

    download_files(&plan).await?;

    let primary_path = model_dir.join(&files[0]);
    let all_paths: Vec<_> = files.iter().map(|f| model_dir.join(f)).collect();

    gglib_core::telemetry::console_println(&format!(
        "✓ Successfully downloaded {} to {}",
        request.model_id,
        model_dir.display()
    ));

    Ok(CliDownloadResult {
        downloaded_paths: all_paths,
        primary_path,
        quantization: quant.clone(),
        repo_id: request.model_id,
        commit_sha,
    })
}

/// Check if a model has an update available, asking the Hub with `token`
/// when there is one.
///
/// `has_update` is true when no `current_sha` is recorded: there is no
/// baseline to compare against, so the caller cannot claim the model is
/// current. Callers that surface this to a user should distinguish that case
/// from a genuine new revision.
pub async fn check_update(
    repo_id: &str,
    current_sha: Option<&str>,
    token: Option<String>,
) -> Result<UpdateCheckResult> {
    check_update_with(&super::api::hub_client(token), repo_id, current_sha).await
}

/// [`check_update`] against a given hub. Fails when the hub answers with no
/// commit.
async fn check_update_with(
    hub: &dyn HfClientPort,
    repo_id: &str,
    current_sha: Option<&str>,
) -> Result<UpdateCheckResult> {
    let latest_sha = hub
        .get_commit_sha(repo_id)
        .await
        .map_err(|e| anyhow!("Failed to get repo info: {e}"))?;
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

#[cfg(test)]
#[path = "check_update_tests.rs"]
mod tests;
