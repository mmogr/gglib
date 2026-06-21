//! Check updates handler.
//!
//! Checks for updates to locally downloaded models.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::handlers::model::resolver;

/// Execute the check-updates command.
///
/// Checks if locally downloaded models have updates available on HuggingFace.
pub async fn execute(ctx: &CliContext, identifier: Option<&str>, all: bool) -> Result<()> {
    if all {
        println!("Checking updates for all models...");
        let models = ctx.app.models().list().await?;

        if models.is_empty() {
            println!("No models found in database.");
            return Ok(());
        }

        for model in models {
            if let Some(hf_repo) = &model.hf_repo_id {
                check_model_update(&model, hf_repo).await?;
            } else {
                println!(
                    "Model '{}' is not from HuggingFace, skipping update check.",
                    model.name
                );
            }
        }
    } else if let Some(ident) = identifier {
        let model = resolver::resolve_model_identifier(ctx, ident).await?;
        if let Some(hf_repo) = &model.hf_repo_id {
            check_model_update(&model, hf_repo).await?;
        } else {
            println!(
                "Model '{}' is not from HuggingFace, cannot check for updates.",
                model.name
            );
        }
    } else {
        println!("Please specify --identifier <id|name> or --all to check for updates.");
    }

    Ok(())
}

/// Check if a single model needs updates.
async fn check_model_update(model: &gglib_core::domain::Model, hf_repo: &str) -> Result<()> {
    use gglib_core::paths::resolve_models_dir;

    println!("Checking updates for: {}", model.name);

    let models_dir = resolve_models_dir(None)?.path;
    let cache_dir = models_dir.join(".cache");

    let api = hf_hub::api::sync::ApiBuilder::new()
        .with_cache_dir(cache_dir)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create HF API client: {}", e))?;

    let repo = api.repo(hf_hub::Repo::with_revision(
        hf_repo.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    match repo.info() {
        Ok(repo_info) => {
            let latest_sha = repo_info.sha;

            if let Some(stored_sha) = &model.hf_commit_sha {
                if *stored_sha == latest_sha {
                    println!("  ✓ Model is up to date (SHA: {})", &latest_sha[..8]);
                } else {
                    println!("  🔄 Update available!");
                    println!("    Current SHA: {}", &stored_sha[..8]);
                    println!("    Latest SHA:  {}", &latest_sha[..8]);
                    println!("    Use: gglib model upgrade {} to update", model.id);
                }
            } else {
                println!("  ⚠️  No commit SHA stored, cannot check for updates");
            }
        }
        Err(e) => {
            println!("  ❌ Failed to check repository: {}", e);
        }
    }

    Ok(())
}
