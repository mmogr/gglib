//! Update model handler.
//!
//! Upgrades a locally downloaded model to the latest `HuggingFace` revision.
//! The check, the download and the row rewrite all live in
//! [`ModelOps::check_upgrade`]/[`ModelOps::apply_upgrade`], the single shared
//! implementation consumed by this CLI, the Axum `WebUI` and the Tauri app.
//! What stays here is what only a terminal has: the plan, the prompt, the
//! printed result, and the download board the upgrade's row is drawn on.
//!
//! [`ModelOps::check_upgrade`]: gglib_app_services::ModelOps::check_upgrade
//! [`ModelOps::apply_upgrade`]: gglib_app_services::ModelOps::apply_upgrade

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::handlers::model::resolver;
use crate::presentation::short_sha;
use crate::utils::input;

use super::board::SoloBoard;

/// Execute the update-model command.
///
/// Upgrades a model to the latest revision from `HuggingFace`. `force` skips
/// the confirmation prompt; everything else is identical to the GUI path.
pub(crate) async fn execute(ctx: &CliContext, identifier: &str, force: bool) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // `NoopModelRuntime` rather than `ctx.runner`: a one-shot CLI command has
    // no shared `ProcessManager`, and the upgrade path never touches serving
    // status. Same construction as `model capabilities`.
    let ops = crate::handlers::model::one_shot_model_ops(ctx);

    println!("Updating model {} (ID: {})...", model.name, model.id);
    if let Some(repo) = model.hf_repo_id.as_deref() {
        println!("  Repository: {repo}");
    }
    if let Some(quant) = model.quantization.as_deref() {
        println!("  Quantization: {quant}");
    }

    let check = ops.check_upgrade(model.id).await?;

    if !check.has_update {
        println!(
            "✓ Model is already up to date (SHA: {})",
            short_sha(&check.latest_sha)
        );
        return Ok(());
    }

    match check.current_sha.as_deref() {
        Some(current) => println!("  Current SHA: {}", short_sha(current)),
        // No baseline recorded, so `has_update` above could not have said
        // otherwise. Say that rather than implying a new release exists.
        None => println!("  Current SHA: none recorded (cannot tell what changed)"),
    }
    println!("  Latest SHA:  {}", short_sha(&check.latest_sha));

    // Confirmation prompt if not forced — this re-downloads the full model
    // and overwrites the stored file path.
    if !force {
        println!();
        println!("This will:");
        println!("  • Re-download the model at the latest revision");
        println!("  • Replace the current file and update the database row");
        println!();
        if !input::prompt_confirmation("Proceed?")? {
            println!("Upgrade cancelled.");
            return Ok(());
        }
    }

    // The upgrade is not on the download queue, so it hands over its own
    // row, and the board draws it as it draws a queued download's, until
    // the upgrade is over.
    let board = SoloBoard::new(std::sync::Arc::clone(&ctx.console));
    let outcome = board
        .during(|rows| ops.apply_upgrade(model.id, Some(rows)))
        .await?;

    if outcome.updated {
        println!("✓ Model updated successfully");
        println!("  New SHA: {}", short_sha(&outcome.latest_sha));
    } else {
        // The revision moved back under us between check and apply.
        println!(
            "✓ Model is already up to date (SHA: {})",
            short_sha(&outcome.latest_sha)
        );
    }

    Ok(())
}
