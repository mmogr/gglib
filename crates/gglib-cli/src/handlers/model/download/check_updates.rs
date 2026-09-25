//! Check updates handler.
//!
//! Checks for updates to locally downloaded models.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::handlers::model::resolver;

use super::update_model::short_sha;

/// Execute the check-updates command.
///
/// Checks if locally downloaded models have updates available on HuggingFace.
pub(crate) async fn execute(ctx: &CliContext, identifier: Option<&str>, all: bool) -> Result<()> {
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

/// Check if a single model needs updates, asking the Hub with the `HF_TOKEN`
/// in the environment when it is set.
async fn check_model_update(model: &gglib_core::domain::Model, hf_repo: &str) -> Result<()> {
    println!("Checking updates for: {}", model.name);

    let check = gglib_download::cli_exec::check_update(
        hf_repo,
        model.hf_commit_sha.as_deref(),
        std::env::var("HF_TOKEN").ok(),
    )
    .await;

    match check {
        Ok(check) => {
            let latest_sha = short_sha(&check.latest_sha);
            if let Some(stored_sha) = &check.current_sha {
                if check.has_update {
                    println!("  🔄 Update available!");
                    println!("    Current SHA: {}", short_sha(stored_sha));
                    println!("    Latest SHA:  {latest_sha}");
                    println!("    Use: gglib model upgrade {} to update", model.id);
                } else {
                    println!("  ✓ Model is up to date (SHA: {latest_sha})");
                }
            } else {
                println!("  ⚠️  No commit SHA stored, cannot check for updates");
            }
        }
        Err(e) => {
            println!("  ✗ Failed to check repository: {}", e);
        }
    }

    Ok(())
}
