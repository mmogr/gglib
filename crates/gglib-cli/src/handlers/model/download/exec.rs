//! Download handler — lean orchestrator.
//!
//! Queues the initial model via [`DownloadManagerPort::queue_smart`] (the same
//! path used by the GUI) and then delegates to [`interactive::run_interactive_monitor`]
//! for progress rendering and optional interactive queue management.
//!
//! Model registration after download is handled internally by the download
//! manager via the shared [`ModelRegistrarPort`], giving full parity with
//! the GUI registration path.

use std::sync::Arc;

use anyhow::Result;
use gglib_download::cli_exec::list_quantizations;

use crate::bootstrap::CliContext;
use gglib_core::paths::resolve_models_dir;

use super::interactive;

/// Download command arguments passed from CLI.
pub struct DownloadArgs<'a> {
    pub model_id: &'a str,
    pub quantization: Option<&'a str>,
    pub list_quants: bool,
    pub force: bool,
    /// HuggingFace token for private models.
    ///
    /// Used only for `--list-quants`. For downloads, prefer the `HF_TOKEN`
    /// environment variable which is read at startup and wired into the
    /// download manager config, mirroring how the GUI handles authentication.
    pub token: Option<&'a str>,
}

/// Execute the download command.
///
/// Queues `model_id` via the shared [`DownloadManagerPort`] and enters the
/// interactive monitor loop. The monitor exits when all queued downloads
/// complete or the user presses `[q]`.
pub async fn execute(ctx: &CliContext, args: DownloadArgs<'_>) -> Result<()> {
    let models_dir = resolve_models_dir(None)?.path;

    // --list-quants: show available quantizations and exit (uses cli_exec directly).
    if args.list_quants {
        list_quantizations(args.model_id, &models_dir, args.token.map(String::from)).await?;
        return Ok(());
    }

    // Queue the initial download via the shared manager (same code path as GUI).
    let quant = args.quantization.map(String::from);
    Arc::clone(&ctx.downloads)
        .queue_smart(args.model_id.to_string(), quant)
        .await?;

    // Hand off to the interactive monitor — all progress rendering, keypress
    // handling, TTY/non-TTY detection, and failure reporting live there.
    interactive::run_interactive_monitor(
        Arc::clone(&ctx.downloads),
        Arc::clone(&ctx.download_emitter),
    )
    .await
}
