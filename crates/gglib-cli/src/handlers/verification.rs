//! Model verification handlers.
//!
//! Handlers for verifying model integrity, checking for updates via OID comparison,
//! and repairing corrupt models.

use anyhow::Result;
use std::time::Instant;

use crate::bootstrap::CliContext;

/// Execute the verify command.
///
/// Verifies model integrity by computing SHA256 hashes and comparing against
/// stored OIDs from HuggingFace.
pub async fn execute_verify(ctx: &CliContext, model_id: i64, verbose: bool) -> Result<()> {
    // Get verification service
    let verification = ctx.app().verification()
        .ok_or_else(|| anyhow::anyhow!("Verification service not available"))?;

    // Get model info for display
    let model = ctx.app().models().get_by_id(model_id).await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", model_id))?;

    println!("🔍 Verifying model: {}", model.name);
    println!();

    let start = Instant::now();

    // Start verification
    let (mut progress_rx, handle) = verification.verify_model_integrity(model_id).await
        .map_err(|e| anyhow::anyhow!("Failed to start verification: {}", e))?;

    // Process progress updates
    while let Some(progress) = progress_rx.recv().await {
        if verbose {
            use gglib_core::services::ShardProgress;
            match &progress.shard_progress {
                ShardProgress::Starting => {
                    println!(
                        "  Shard {}/{}: Starting verification...",
                        progress.shard_index + 1,
                        progress.total_shards
                    );
                }
                ShardProgress::Hashing { percent, bytes_processed, total_bytes } => {
                    let mb_processed = *bytes_processed as f64 / 1024.0 / 1024.0;
                    let mb_total = *total_bytes as f64 / 1024.0 / 1024.0;
                    println!(
                        "  Shard {}/{}: Hashing... {}% ({:.1} MB / {:.1} MB)",
                        progress.shard_index + 1,
                        progress.total_shards,
                        percent,
                        mb_processed,
                        mb_total
                    );
                }
                ShardProgress::Completed { health } => {
                    use gglib_core::services::ShardHealth;
                    let status = match health {
                        ShardHealth::Healthy => "✓ Healthy".to_string(),
                        ShardHealth::Corrupt { expected, actual } => {
                            format!("✗ Corrupt (expected: {}, actual: {})", &expected[..8], &actual[..8])
                        }
                        ShardHealth::Missing => "✗ Missing".to_string(),
                        ShardHealth::NoOid => "⚠ No OID available".to_string(),
                    };
                    println!(
                        "  Shard {}/{}: {}",
                        progress.shard_index + 1,
                        progress.total_shards,
                        status
                    );
                }
            }
        }
    }

    // Wait for completion and get report
    let report = handle.await
        .map_err(|e| anyhow::anyhow!("Verification task failed: {}", e))??;

    let elapsed = start.elapsed();

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("Verification completed in {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("Overall Health: {}", match report.overall_health {
        gglib_core::services::OverallHealth::Healthy => "✓ Healthy",
        gglib_core::services::OverallHealth::Unhealthy => "✗ Unhealthy",
        gglib_core::services::OverallHealth::Unverifiable => "⚠ Unverifiable",
    });
    println!();

    // Show shard details
    for shard in &report.shards {
        use gglib_core::services::ShardHealth;
        let status = match &shard.health {
            ShardHealth::Healthy => "✓".to_string(),
            ShardHealth::Corrupt { .. } => "✗".to_string(),
            ShardHealth::Missing => "✗".to_string(),
            ShardHealth::NoOid => "⚠".to_string(),
        };
        println!("  {} Shard {}: {}", status, shard.index, shard.file_path);
        
        if let ShardHealth::Corrupt { expected, actual } = &shard.health {
            println!("      Expected: {}", expected);
            println!("      Actual:   {}", actual);
        }
    }

    println!("═══════════════════════════════════════════════════════════");

    // Return error if unhealthy
    if matches!(report.overall_health, gglib_core::services::OverallHealth::Unhealthy) {
        println!();
        println!("⚠️  Model has integrity issues. Run 'gglib repair {}' to fix.", model_id);
        std::process::exit(1);
    }

    Ok(())
}

/// Execute the repair command.
///
/// Repairs a corrupt model by deleting failed shards and re-downloading them.
pub async fn execute_repair(
    ctx: &CliContext,
    model_id: i64,
    shards: Option<String>,
    force: bool,
) -> Result<()> {
    // Get verification service
    let verification = ctx.app().verification()
        .ok_or_else(|| anyhow::anyhow!("Verification service not available"))?;

    // Get model info for display
    let model = ctx.app().models().get_by_id(model_id).await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", model_id))?;

    println!("🔧 Repairing model: {}", model.name);

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

    // Confirmation prompt if not forced
    if !force {
        println!();
        println!("This will:");
        println!("  • Delete corrupt or missing model files");
        println!("  • Re-download them from HuggingFace");
        println!();
        print!("Proceed? (y/N): ");
        
        use std::io::{self, Write};
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Repair cancelled.");
            return Ok(());
        }
    }

    println!();
    println!("Starting repair...");

    // Execute repair
    verification.repair_model(model_id, shard_indices).await
        .map_err(|e| anyhow::anyhow!("Repair failed: {}", e))?;

    println!("✓ Repair completed successfully");
    println!();
    println!("Note: The model files have been queued for re-download.");
    println!("      Use 'gglib verify {}' to check status after download completes.", model_id);

    Ok(())
}
