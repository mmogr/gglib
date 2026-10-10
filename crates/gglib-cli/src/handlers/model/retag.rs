//! Retag command handler.
//!
//! Re-derives auto-generated tags (capability flags + `format:*` dialect
//! tags) for one or more models from their persisted GGUF metadata, which
//! backfills the `format:*` tags on a catalog row that lacks them.
//!
//! Default behaviour is additive: missing tags are appended, nothing is
//! removed. `--full` drops and re-derives the entire auto-generated
//! namespace while still preserving user-curated tags. A model with no
//! image family has one read from its file's tensor names when it names one.
//!
//! Each model is retagged through `ModelOps::retag`, the operation the
//! app's retag runs.

use anyhow::{Context, Result};
use gglib_app_services::ModelOps;

use super::resolver;
use crate::bootstrap::CliContext;

/// Execute the retag command.
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    identifier: Option<String>,
    all: bool,
    full: bool,
) -> Result<()> {
    let targets = if all {
        ctx.app
            .models()
            .list()
            .await
            .context("failed to list models")?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect::<Vec<_>>()
    } else if let Some(id) = identifier {
        let m = resolver::resolve_model_identifier(ctx, &id).await?;
        vec![(m.id, m.name)]
    } else {
        anyhow::bail!("specify a model identifier or pass --all");
    };

    if targets.is_empty() {
        println!("No models to retag.");
        return Ok(());
    }

    let mode = if full { "full rebuild" } else { "additive" };
    println!("Retagging {} model(s) ({mode}) ...", targets.len());

    let mut total_changed = 0usize;
    for (id, name) in targets {
        match ops.retag(id, full).await {
            Ok(pass) if !pass.changed => {
                println!("  [{id}] {name} — already up to date");
            }
            Ok(pass) => {
                total_changed += 1;
                if !pass.added.is_empty() {
                    println!("  [{id}] {name} — added: {}", pass.added.join(", "));
                }
                if !pass.removed.is_empty() {
                    println!("  [{id}] {name} — removed: {}", pass.removed.join(", "));
                }
                if pass.spec_changed {
                    println!("  [{id}] {name} — dialect spec re-derived");
                }
                if let Some(family) = pass.family_found {
                    println!("  [{id}] {name} — draws as {}", family.label());
                }
            }
            Err(e) => {
                eprintln!("  [{id}] {name} — FAILED: {e}");
            }
        }
    }

    println!("Done. {total_changed} model(s) updated.");
    Ok(())
}
