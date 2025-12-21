//! Download handler.
//!
//! Downloads models from HuggingFace Hub and registers them in the database.

use anyhow::Result;
use chrono::Utc;
use gglib_core::domain::NewModel;
use gglib_download::cli_exec::{self, CliDownloadRequest, list_quantizations};

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

    // Create model name from repo_id
    let model_name = result
        .repo_id
        .split('/')
        .next_back()
        .unwrap_or(&result.repo_id)
        .to_string();

    // Build NewModel for database registration
    let mut new_model = NewModel::new(
        format!("{}-{}", model_name, result.quantization),
        result.primary_path.clone(),
        0.0, // Will be populated from GGUF metadata during add
        Utc::now(),
    );
    new_model.hf_repo_id = Some(result.repo_id.clone());
    new_model.hf_commit_sha = Some(result.commit_sha.clone());
    new_model.hf_filename = result
        .primary_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    new_model.quantization = Some(result.quantization.clone());
    new_model.download_date = Some(Utc::now());

    match ctx.app().models().add(new_model).await {
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
                "  You can manually add the model with: gglib add {}",
                result.primary_path.display()
            );
        }
    }

    Ok(())
}
