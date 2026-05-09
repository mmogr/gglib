//! Retag command handler.
//!
//! Re-derives auto-generated tags (capability flags + `format:*` dialect
//! tags) for one or more models from their persisted GGUF metadata. Used
//! to backfill the new `format:*` tags introduced by the universal
//! normalization pipeline on catalogs that pre-date that change.
//!
//! Default behaviour is additive: missing tags are appended, nothing is
//! removed. `--full` drops and re-derives the entire auto-generated
//! namespace while still preserving user-curated tags.

use anyhow::{Context, Result};

use crate::bootstrap::CliContext;

/// Execute the retag command.
pub async fn execute(
    ctx: &CliContext,
    identifier: Option<String>,
    all: bool,
    full: bool,
) -> Result<()> {
    let models = ctx.app.models();
    let parser = ctx.gguf_parser.as_ref();

    let targets = if all {
        models
            .list()
            .await
            .context("failed to list models")?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect::<Vec<_>>()
    } else if let Some(id) = identifier {
        let m = models
            .find_by_identifier(&id)
            .await
            .context("failed to look up model")?;
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
        match models.retag_model(id, parser, full).await {
            Ok(None) => {
                println!("  [{id}] {name} — already up to date");
            }
            Ok(Some(diff)) => {
                total_changed += 1;
                if !diff.added.is_empty() {
                    println!("  [{id}] {name} — added: {}", diff.added.join(", "));
                }
                if !diff.removed.is_empty() {
                    println!("  [{id}] {name} — removed: {}", diff.removed.join(", "));
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
