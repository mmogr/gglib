//! Download handler — lean orchestrator.
//!
//! Queues the download on the gglib daemon (`POST /api/models/downloads/queue`,
//! the same route the GUI uses) and watches the daemon's queue for progress.
//! Before it queues an image model it says what companions the download
//! brings, with `companions`.
//! The daemon owns the download and registers the model when it completes, so
//! detaching this command does not interrupt anything.

use std::sync::Arc;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::status_line::hub_wait_notice;

use super::{companions, list_quants, remote};

/// Download command arguments passed from CLI.
pub(crate) struct DownloadArgs<'a> {
    pub model_id: &'a str,
    pub quantization: Option<&'a str>,
    pub list_quants: bool,
}

/// Execute the download command.
///
/// Queues `model_id` on the daemon and watches the queue until the download
/// the daemon answered with has ended. A queue Hugging Face's rate limit
/// refused is asked again once it lifts, the wait on the status line.
/// Ctrl-C detaches; the daemon keeps downloading and registers the model
/// itself.
pub(crate) async fn execute(ctx: &CliContext, args: DownloadArgs<'_>) -> Result<()> {
    // --list-quants: the daemon's listing, printed, and nothing queued.
    if args.list_quants {
        return list_quants::execute(ctx, args.model_id).await;
    }

    let handle = daemon_client::ensure_daemon().await?;
    let listing = companions::listing_ops(ctx, None);
    if let Some(preview) = companions::preview(&listing, args.model_id).await {
        ctx.console.println(preview.trim_end());
    }
    let body = daemon_client::QueueDownloadBody {
        model_id: args.model_id.to_string(),
        quantization: args.quantization.map(String::from),
    };
    // The daemon answers a queue the Hub's rate limit refused at once; this
    // command waits it out on its status line and asks again.
    let waiting = hub_wait_notice(Arc::clone(&ctx.console));
    let queue = handle.queue_download(&body, &waiting);
    remote::monitor(&handle, Arc::clone(&ctx.console), queue).await
}
