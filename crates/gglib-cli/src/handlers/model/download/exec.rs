//! Download handler — lean orchestrator.
//!
//! Queues the download on the gglib daemon (`POST /api/models/downloads/queue`,
//! the same route the GUI uses) and watches the daemon's queue for progress.
//! The daemon owns the download and registers the model when it completes, so
//! detaching this command does not interrupt anything.

use anyhow::Result;
use gglib_download::cli_exec::list_quantizations;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use gglib_core::paths::resolve_models_dir;

use super::remote;

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
/// Queues `model_id` on the daemon and watches the queue until it drains.
/// Ctrl-C detaches; the daemon keeps downloading and registers the model
/// itself.
pub async fn execute(ctx: &CliContext, args: DownloadArgs<'_>) -> Result<()> {
    let _ = ctx;
    let models_dir = resolve_models_dir(None)?.path;

    // --list-quants: show available quantizations and exit (uses cli_exec directly).
    if args.list_quants {
        list_quantizations(args.model_id, &models_dir, args.token.map(String::from)).await?;
        return Ok(());
    }

    let handle = daemon_client::ensure_daemon().await?;
    remote::queue(&handle, args.model_id, args.quantization.map(String::from)).await?;
    remote::monitor(&handle).await
}
