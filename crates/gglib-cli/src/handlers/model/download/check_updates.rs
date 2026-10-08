//! Check updates handler.
//!
//! Checks for updates to locally downloaded models. The comparison is
//! [`ModelOps::check_update`], the one `gglib model upgrade` and the daemon's
//! upgrade check make; what is here is which models are asked about, and the
//! words printed.
//!
//! [`ModelOps::check_update`]: gglib_app_services::ModelOps::check_update

use anyhow::Result;
use gglib_app_services::ModelOps;

use crate::bootstrap::CliContext;
use crate::handlers::model::{one_shot_model_ops, resolver};
use crate::presentation::short_sha;

/// Execute the check-updates command.
///
/// Checks if locally downloaded models have updates available on `HuggingFace`.
pub(crate) async fn execute(ctx: &CliContext, identifier: Option<&str>, all: bool) -> Result<()> {
    let ops = one_shot_model_ops(ctx);
    if all {
        println!("Checking updates for all models...");
        let models = ctx.app.models().list().await?;

        if models.is_empty() {
            println!("No models found in database.");
            return Ok(());
        }

        for model in models {
            if model.hf_repo_id.is_some() {
                check_model_update(&ops, &model).await;
            } else {
                println!(
                    "Model '{}' is not from HuggingFace, skipping update check.",
                    model.name
                );
            }
        }
    } else if let Some(ident) = identifier {
        let model = resolver::resolve_model_identifier(ctx, ident).await?;
        if model.hf_repo_id.is_some() {
            check_model_update(&ops, &model).await;
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

/// Check if a single model needs updates, and print what was found. The Hub
/// is asked with the token the core was built with.
async fn check_model_update(ops: &ModelOps, model: &gglib_core::domain::Model) {
    println!("Checking updates for: {}", model.name);

    match ops.check_update(model.id).await {
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
            println!("  ✗ Failed to check repository: {e}");
        }
    }
}
