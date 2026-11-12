//! Model verification handlers.
//!
//! Handlers for verifying model integrity, checking for updates via OID comparison,
//! and repairing corrupt models.

use anyhow::{Context as _, Result};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use gglib_core::services::{OverallHealth, ShardHealth, ShardHealthReport, VerificationReport};

use super::{download, resolver};
use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::short_sha;
use crate::utils::input;

/// Execute the verify command.
///
/// Verifies model integrity by computing SHA256 hashes and comparing against
/// stored OIDs from `HuggingFace`.
///
/// `per_shard` prints a line per file as it is hashed. Each file is named by
/// what it is: `Shard i/n`, counted over the weights alone, `Projector`, or
/// `Companion`; the summary adds its file index, which `repair --shards`
/// takes. It is deliberately not
/// spelled `--verbose`: that id belongs to the global logging flag, and a
/// local one by the same name stops the global from ever reaching this
/// command.
pub(crate) async fn execute_verify(
    ctx: &CliContext,
    identifier: &str,
    per_shard: bool,
) -> Result<()> {
    let verification = ctx.app.verification();

    // Resolve name-or-id to a model record.
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    println!("\u{1f50d} Verifying model: {}", model.name);
    println!();

    let start = Instant::now();

    // Start verification
    let (mut progress_rx, handle) = verification
        .verify_model_integrity(model.id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start verification: {e}"))?;

    // Process progress updates
    while let Some(progress) = progress_rx.recv().await {
        if per_shard {
            use gglib_core::services::ShardProgress;
            match &progress.shard_progress {
                ShardProgress::Starting => {
                    println!("  {}: Starting verification...", progress.kind.label());
                }
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "display only: a byte count shown in MiB to one decimal"
                )]
                ShardProgress::Hashing {
                    percent,
                    bytes_processed,
                    total_bytes,
                } => {
                    let mib_processed = *bytes_processed as f64 / 1024.0 / 1024.0;
                    let mib_total = *total_bytes as f64 / 1024.0 / 1024.0;
                    println!(
                        "  {}: Hashing... {}% ({:.1} MiB / {:.1} MiB)",
                        progress.kind.label(),
                        percent,
                        mib_processed,
                        mib_total
                    );
                }
                ShardProgress::Completed { health } => {
                    println!("  {}: {}", progress.kind.label(), file_status(health));
                }
            }
        }
    }

    // Wait for completion and get report
    let report = handle
        .await
        .map_err(|e| anyhow::anyhow!("Verification task failed: {e}"))??;

    let elapsed = start.elapsed();

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("Verification completed in {:.1}s", elapsed.as_secs_f64());
    println!();
    print!("{}", summary(&report));
    println!("═══════════════════════════════════════════════════════════");

    if fails(&report.overall_health) {
        println!();
        println!(
            "⚠️  Model has integrity issues. Run 'gglib model repair {}' to fix.",
            model.name
        );
        std::process::exit(1);
    }

    Ok(())
}

/// Whether `gglib model verify` exits 1 on this verdict: only when a file
/// is corrupt or missing. A model with nothing to compare does not fail.
pub(super) fn fails(health: &OverallHealth) -> bool {
    *health == OverallHealth::Unhealthy
}

/// The report under the run's timing: the verdict's line, with how many
/// files are damaged when any is, then a row per file, a corrupt file's two
/// hashes under it.
pub(super) fn summary(report: &VerificationReport) -> String {
    let mut out = overall_line(&report.overall_health);
    if let Some(damage) = &report.damage {
        out.push_str(" · ");
        out.push_str(&damage.line);
    }
    out.push_str("\n\n");
    for shard in &report.shards {
        out.push_str(&summary_row(shard));
        out.push('\n');
        if let ShardHealth::Corrupt { expected, actual } = &shard.health {
            let _ = writeln!(out, "      Expected: {expected}\n      Actual:   {actual}");
        }
    }
    out
}

/// The verdict's line: `Overall Health: ⚠ Unverifiable`.
pub(super) fn overall_line(health: &OverallHealth) -> String {
    let mark = match health {
        OverallHealth::Healthy => "✓",
        OverallHealth::Unhealthy => "✗",
        OverallHealth::Unverifiable => "⚠",
    };
    format!("Overall Health: {mark} {}", health.word())
}

/// A file's row in the summary: `  ✓ Shard 1/2: path (file 0)`.
pub(super) fn summary_row(shard: &ShardHealthReport) -> String {
    format!(
        "  {} {}: {} (file {})",
        file_mark(&shard.health),
        shard.kind.label(),
        shard.file_path,
        shard.index
    )
}

/// The mark a file's health is printed with: ✓ matched, ✗ damaged, ⚠ not
/// compared.
fn file_mark(health: &ShardHealth) -> &'static str {
    match health {
        ShardHealth::Healthy => "✓",
        ShardHealth::Corrupt { .. } | ShardHealth::Missing => "✗",
        ShardHealth::NoOid => "⚠",
    }
}

/// A file's health as `--per-shard` prints it once the file is checked: the
/// mark and the word, and a corrupt file's two hashes, shortened.
pub(super) fn file_status(health: &ShardHealth) -> String {
    let said = format!("{} {}", file_mark(health), health.word());
    match health {
        ShardHealth::Corrupt { expected, actual } => format!(
            "{said} (expected: {}, actual: {})",
            short_sha(expected),
            short_sha(actual)
        ),
        _ => said,
    }
}

/// Execute the repair command.
///
/// Repairs a corrupt model by deleting failed shards and re-downloading them.
/// The daemon does both, on the queue `gglib model download` uses, and this
/// command watches the download to its end: it succeeds when the files are
/// back, and otherwise fails with what is missing and what fetches it.
/// Ctrl-C detaches, and the daemon keeps downloading.
pub(crate) async fn execute_repair(
    ctx: &CliContext,
    identifier: &str,
    shards: Option<String>,
    yes: bool,
) -> Result<()> {
    // Resolve name-or-id to a model record.
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    println!("\u{1f527} Repairing model: {}", model.name);

    // Parse shard indices if provided
    let shard_indices = if let Some(shard_str) = shards {
        let indices: Result<Vec<usize>, _> = shard_str
            .split(',')
            .map(|s| s.trim().parse::<usize>())
            .collect();
        Some(indices?)
    } else {
        None
    };

    // Confirmation prompt unless --yes answered already
    if !yes {
        println!();
        println!("This will:");
        println!("  • Delete corrupt or missing model files");
        println!("  • Re-download them from HuggingFace");
        println!();
        if !input::prompt_confirmation("Proceed?")? {
            println!("Repair cancelled.");
            return Ok(());
        }
    }

    println!();
    println!("Starting repair...");

    let folder = model
        .file_path
        .parent()
        .context("the model's file has no folder")?;
    let handle = daemon_client::ensure_daemon().await?;
    let repair = handle.repair_model(model.id, shard_indices);
    download::monitor_repair(&handle, Arc::clone(&ctx.console), folder, repair).await
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;
