//! Download handler.
//!
//! Downloads models from HuggingFace Hub and registers them in the database
//! via [`ModelRegistrarPort`], the same registration path used by the GUI.

use std::str::FromStr;

use anyhow::Result;
use gglib_core::download::Quantization;
use gglib_core::ports::CompletedDownload;
use gglib_download::cli_exec::{self, CliDownloadRequest, CliDownloadResult, list_quantizations};

use crate::bootstrap::CliContext;
use gglib_core::paths::resolve_models_dir;

/// Download command arguments passed from CLI.
pub struct DownloadArgs<'a> {
    pub model_id: &'a str,
    pub quantization: Option<&'a str>,
    pub list_quants: bool,
    pub force: bool,
    pub token: Option<&'a str>,
}

/// Execute the download command.
///
/// Downloads a model from HuggingFace and registers it in the database.
pub async fn execute(ctx: &CliContext, args: DownloadArgs<'_>) -> Result<()> {
    let models_dir = resolve_models_dir(None)?.path;

    // If --list-quants, just show available quantizations and exit
    if args.list_quants {
        list_quantizations(args.model_id, &models_dir, args.token.map(String::from)).await?;
        return Ok(());
    }

    // Build download request
    let mut request = CliDownloadRequest::new(args.model_id, models_dir.clone())
        .with_force(args.force)
        .with_token(args.token.map(String::from));

    if let Some(quant) = args.quantization {
        request = request.with_quantization(quant);
    }

    // Execute download (this handles progress display)
    let result = cli_exec::download(request).await?;

    // Register via the same ModelRegistrarPort that the GUI uses,
    // ensuring full GGUF metadata extraction and parity.
    let completed = build_completed_download(&result)?;

    match ctx.model_registrar.register_model(&completed).await {
        Ok(model) => {
            println!("✓ Model registered in database:");
            println!("  ID: {}", model.id);
            println!("  Name: {}", model.name);
            println!("  Path: {}", model.file_path.display());
        }
        Err(e) => {
            // Don't fail - the download succeeded, just registration failed
            println!(
                "⚠️  Download succeeded but database registration failed: {}",
                e
            );
            println!(
                "  You can manually add the model with: gglib model add {}",
                result.primary_path.display()
            );
        }
    }

    Ok(())
}

/// Map a [`CliDownloadResult`] to the [`CompletedDownload`] DTO that
/// [`ModelRegistrarPort::register_model`] expects.
fn build_completed_download(result: &CliDownloadResult) -> Result<CompletedDownload> {
    let quantization = Quantization::from_str(&result.quantization)
        .unwrap_or_default(); // falls back to Quantization::Unknown

    let is_sharded = result.downloaded_paths.len() > 1;

    let total_bytes = result
        .downloaded_paths
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    let file_paths = if is_sharded {
        Some(result.downloaded_paths.clone())
    } else {
        None
    };

    Ok(CompletedDownload {
        primary_path: result.primary_path.clone(),
        all_paths: result.downloaded_paths.clone(),
        quantization,
        repo_id: result.repo_id.clone(),
        commit_sha: result.commit_sha.clone(),
        is_sharded,
        total_bytes,
        file_paths,
        hf_tags: vec![],
        hf_file_entries: vec![],
    })
}
