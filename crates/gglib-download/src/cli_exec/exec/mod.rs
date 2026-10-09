#![doc = include_str!("README.md")]
pub(crate) mod python_bridge;
pub(crate) mod python_env;
mod python_protocol;
mod python_requirements;

use std::fs;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use gglib_core::download::{DownloadError, DownloadId};
use gglib_core::ports::{HfClientPort, QuantizationResolver, ResolvedFile};

use super::types::{CliDownloadRequest, CliDownloadResult, CliUpdateRequest, UpdateCheckResult};
use super::utils::model_directory;
use crate::executor::{DownloadPlan, download_file};
use crate::manager::PROGRESS_TICK;
use crate::resolver::HfQuantizationResolver;
use crate::solo::{RowCallback, fetch_solo};

/// Execute a download request and return the result, handing `rows` the
/// download's row while its files are fetched.
///
/// Used internally by [`update_model`] for the force-redownload path.
/// Interactive CLI downloads now route through
/// [`DownloadManagerPort::queue_smart`](gglib_core::ports::DownloadManagerPort::queue_smart)
/// instead of calling this function directly.
pub(super) async fn download(
    request: CliDownloadRequest,
    rows: Option<RowCallback>,
) -> Result<CliDownloadResult> {
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
    // A forced fetch again of the model's own files: its companions, if it
    // draws with any, are in their own folders and linked already.
    let resolver = HfQuantizationResolver::weights_only(hub);

    let quantization = gglib_core::download::Quantization::from_filename(quant);
    let resolution = resolver.resolve(&request.model_id, quantization).await
        .map_err(|e| anyhow!(
            "No GGUF file found for quantization '{quant}'. Use --list-quants to see available options. Error: {e}",
        ))?;

    // Weights first: `files[0]` is the primary file, never the projector.
    let files: Vec<String> = resolution.files.iter().map(|f| f.path.clone()).collect();
    if resolution.is_sharded {
        gglib_core::telemetry::console_println(&format!(
            "✓ Found {} sharded files for quantization {}",
            resolution.shard_count(),
            quant
        ));
    } else {
        gglib_core::telemetry::console_println(&format!("✓ Found file: {}", files[0]));
    }
    if let Some(projector) = resolution.projector() {
        gglib_core::telemetry::console_println(&format!(
            "✓ Found its projector: {}",
            projector.path
        ));
    }

    // Prepare destination directory
    let model_dir = model_directory(&request.models_dir, &request.model_id);
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    // Download the files one at a time, as one download with one row.
    let id = DownloadId::new(request.model_id.as_str(), Some(quant.as_str()));
    let transfer = Transfer {
        repo_id: &request.model_id,
        revision: &commit_sha,
        destination: &model_dir,
        token: request.token.as_deref(),
        force: request.force,
    };
    let fetch_one = |plan| async move { download_file(&plan).await };
    fetch_files(id, &resolution.files, &transfer, rows.as_ref(), fetch_one).await?;

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

/// What every file of one download is fetched with.
struct Transfer<'a> {
    /// `owner/name` on `HuggingFace`.
    repo_id: &'a str,
    /// The commit the files are read at.
    revision: &'a str,
    /// Directory the files land in.
    destination: &'a Path,
    /// Bearer token for private repositories.
    token: Option<&'a str>,
    /// Re-fetch a file that is already on disk.
    force: bool,
}

/// Fetch the download `id` of `files` with `fetch_one`, one file after
/// another, handing `rows` the download's row while they are fetched.
///
/// Each file's plan carries the sink the row's bytes are read from. It
/// carries a sink for notes only when there is a `rows` to show them on.
/// Without one the plan has none: the accelerator's environment setup then
/// prints its notes to the console, and a note of the transfer's own is not
/// shown.
async fn fetch_files<'a, F, Fut>(
    id: DownloadId,
    files: &'a [ResolvedFile],
    transfer: &Transfer<'a>,
    rows: Option<&RowCallback>,
    fetch_one: F,
) -> Result<(), DownloadError>
where
    F: Fn(DownloadPlan<'a>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<(), DownloadError>> + Send,
{
    let shown = rows.is_some();
    let fetch = |index: usize, progress, notice| {
        let file = &files[index];
        fetch_one(DownloadPlan {
            repo_id: transfer.repo_id,
            revision: transfer.revision,
            destination: transfer.destination,
            file: &file.path,
            token: transfer.token,
            force: transfer.force,
            progress: Some(progress),
            notice: shown.then_some(notice),
            expected_size: file.size,
            cancel: None,
        })
    };
    fetch_solo(id, files, rows, PROGRESS_TICK, fetch).await
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

/// Update a model to the latest version, handing `rows` the download's row
/// while its files are fetched. Without `rows` its progress is not shown.
pub async fn update_model(
    request: CliUpdateRequest,
    rows: Option<RowCallback>,
) -> Result<CliDownloadResult> {
    // Reuse the download logic with force=true
    let download_request = CliDownloadRequest {
        model_id: request.repo_id,
        quantization: Some(request.quantization),
        models_dir: request.models_dir,
        force: true,
        token: request.token,
    };

    download(download_request, rows).await
}

#[cfg(test)]
#[path = "check_update_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "fetch_files_tests.rs"]
mod fetch_files_tests;
