//! Update model handler.
//!
//! Upgrades a locally downloaded model to the latest HuggingFace revision.
//! The check, the download and the row rewrite all live in
//! [`ModelOps::check_upgrade`]/[`ModelOps::apply_upgrade`], the single shared
//! implementation consumed by this CLI, the Axum WebUI and the Tauri app.
//! What stays here is what only a terminal has: the plan, the prompt and the
//! printed result.

use std::sync::Arc;

use anyhow::Result;
use gglib_app_services::{ModelDeps, ModelOps};

use crate::bootstrap::CliContext;
use crate::handlers::model::resolver;

/// Execute the update-model command.
///
/// Upgrades a model to the latest revision from HuggingFace. `force` skips
/// the confirmation prompt; everything else is identical to the GUI path.
pub(crate) async fn execute(ctx: &CliContext, identifier: &str, force: bool) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // `NoopModelRuntime` rather than `ctx.runner`: a one-shot CLI command has
    // no shared `ProcessManager`, and the upgrade path never touches serving
    // status. Same construction as `model capabilities`.
    let ops = ModelOps::new(ModelDeps {
        core: ctx.app.clone(),
        runtime: Arc::new(gglib_core::ports::NoopModelRuntime),
        gguf_parser: ctx.gguf_parser.clone(),
    });

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
        print!("Proceed? (y/N): ");

        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Upgrade cancelled.");
            return Ok(());
        }
    }

    let outcome = ops.apply_upgrade(model.id).await?;

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

/// First 8 characters of a commit SHA, without assuming there are 8.
/// HuggingFace returns 40, but a truncated or empty value must not panic a
/// command whose whole job is repairing a model.
fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::short_sha;

    #[test]
    fn short_sha_truncates_a_full_sha() {
        assert_eq!(
            short_sha("0123456789abcdef0123456789abcdef01234567"),
            "01234567"
        );
    }

    #[test]
    fn short_sha_tolerates_shorter_input() {
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha(""), "");
    }
}
