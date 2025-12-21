//! Update model handler.
//!
//! Updates a locally downloaded model to the latest version from HuggingFace.

use anyhow::{Result, anyhow};
use chrono::Utc;
use gglib_download::cli_exec::{self, CliUpdateRequest};

use crate::bootstrap::CliContext;
use gglib_core::paths::resolve_models_dir;

/// Execute the update-model command.
///
/// Updates a model to the latest version from HuggingFace.
pub async fn execute(ctx: &CliContext, model_id: u32) -> Result<()> {
    let models_dir = resolve_models_dir(None)?.path;

    // Get model from database
    let mut model = ctx
        .app()
        .models()
        .get_by_id(model_id as i64)
        .await?
        .ok_or_else(|| anyhow!("Model with ID {} not found", model_id))?;

    let hf_repo = model
        .hf_repo_id
        .as_ref()
        .ok_or_else(|| anyhow!("Model is not from HuggingFace, cannot update"))?
        .clone();

    let quantization = model
        .quantization
        .as_ref()
        .ok_or_else(|| anyhow!("Model has no quantization info stored"))?
        .clone();

    println!("Updating model {} (ID: {})...", model.name, model_id);
    println!("  Repository: {}", hf_repo);
    println!("  Quantization: {}", quantization);

    // Check if update is available
    let check_result =
        cli_exec::check_update(&hf_repo, model.hf_commit_sha.as_deref(), &models_dir).await?;

    if !check_result.has_update {
        println!(
            "✓ Model is already up to date (SHA: {})",
            &check_result.latest_sha[..8]
        );
        return Ok(());
    }

    if let Some(ref current) = check_result.current_sha {
        println!("  Current SHA: {}", &current[..8]);
    }
    println!("  Latest SHA:  {}", &check_result.latest_sha[..8]);

    // Build update request
    let request = CliUpdateRequest {
        model_path: model.file_path.clone(),
        repo_id: hf_repo,
        quantization,
        models_dir,
        token: None, // TODO: Get from config
    };

    // Execute update
    let result = cli_exec::update_model(request).await?;

    // Update database record by modifying the model directly
    model.file_path = result.primary_path.clone();
    model.hf_commit_sha = Some(result.commit_sha.clone());
    model.last_update_check = Some(Utc::now());

    ctx.app().models().update(&model).await?;

    println!("✓ Model updated successfully");
    println!("  New SHA: {}", &result.commit_sha[..8]);

    Ok(())
}
