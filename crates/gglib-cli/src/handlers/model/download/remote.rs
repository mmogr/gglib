//! Progress monitor for downloads running on the gglib daemon.
//!
//! Presentation only: the queue snapshot arrives from
//! [`DaemonHandle::download_queue`], and this module hands it to the download
//! board. The download itself belongs to the daemon — Ctrl-C here (or a
//! closed terminal) detaches the monitor and the download keeps going, which
//! is the point of daemon ownership.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use gglib_core::download::QueueSnapshot;

use crate::console::CliConsole;
use crate::daemon_client::DaemonHandle;

use super::monitor::{QueueWatch, model_result};

/// Poll interval for queue snapshots. Matches the daemon's own progress
/// sampling tick (250ms), so the bars are at most one tick behind without
/// hammering the loopback API.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Watch the daemon's download queue until the downloads of `model_id` have
/// ended, drawing the queue on `console`.
///
/// Exits successfully when they completed. It is an error when one failed or
/// was cancelled, and when they left the queue with no outcome recorded.
/// Ctrl-C detaches — the daemon keeps downloading.
pub(super) async fn monitor(
    handle: &DaemonHandle,
    console: Arc<CliConsole>,
    model_id: &str,
) -> Result<()> {
    let watch = watch_model(console, model_id, POLL_INTERVAL, || async {
        handle
            .download_queue()
            .await
            .context("polling the daemon download queue")
    });
    tokio::select! {
        result = watch => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!();
            eprintln!("  Detached \u{2014} the download continues on the gglib daemon.");
            eprintln!("  Re-attach anytime with `gglib model download <id>` or the dashboard.");
            Ok(())
        }
    }
}

/// Read the queue with `poll`, once and then again after each wait of
/// `every`, until the downloads of `model_id` have ended. The result is
/// [`model_result`] of how they ended; a read that fails ends the watch with
/// its error.
async fn watch_model<F, Fut>(
    console: Arc<CliConsole>,
    model_id: &str,
    every: Duration,
    mut poll: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<QueueSnapshot>>,
{
    let mut watch = QueueWatch::model(console, model_id);

    loop {
        let snapshot = poll().await?;
        if let Some(ended) = watch.take(&snapshot) {
            return model_result(model_id, &ended).map_err(anyhow::Error::msg);
        }

        tokio::time::sleep(every).await;
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
