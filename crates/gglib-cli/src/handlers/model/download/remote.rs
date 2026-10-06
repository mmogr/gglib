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

use gglib_core::download::{DownloadId, QueueSnapshot};

use crate::console::CliConsole;
use crate::daemon_client::DaemonHandle;

use super::monitor::{QueueWatch, download_result};

/// Poll interval for queue snapshots. Matches the daemon's own progress
/// sampling tick (250ms), so the bars are at most one tick behind without
/// hammering the loopback API.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Wait for `queue`, the request that queues a download on the daemon, and
/// watch the daemon's download queue until the download it answered has
/// ended, drawing the queue on `console`.
///
/// Exits successfully when it completed. It is an error when it failed or
/// was cancelled, and when its outcome is no longer in the queue.
/// Ctrl-C while watching detaches — the daemon keeps downloading.
pub(super) async fn monitor<Q>(
    handle: &DaemonHandle,
    console: Arc<CliConsole>,
    queue: Q,
) -> Result<()>
where
    Q: Future<Output = Result<DownloadId>>,
{
    let poll = || async {
        handle
            .download_queue()
            .await
            .context("polling the daemon download queue")
    };
    let detached = async {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!();
        eprintln!("  Detached \u{2014} the download continues on the gglib daemon.");
        eprintln!("  Re-attach anytime with `gglib model download <id>` or the dashboard.");
    };
    queue_then_watch(console, queue, POLL_INTERVAL, poll, detached).await
}

/// Wait for `queue` to answer a download's id, then watch that download,
/// and no other, with [`watch_download`]. A queue request that fails ends
/// here with its error, and the queue is never read. Once the watch has
/// begun, `detached` completing ends it successfully.
async fn queue_then_watch<Q, F, Fut, D>(
    console: Arc<CliConsole>,
    queue: Q,
    every: Duration,
    poll: F,
    detached: D,
) -> Result<()>
where
    Q: Future<Output = Result<DownloadId>>,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<QueueSnapshot>>,
    D: Future<Output = ()>,
{
    let id = queue.await?;
    tokio::select! {
        result = watch_download(console, &id, every, poll) => result,
        () = detached => Ok(()),
    }
}

/// Read the queue with `poll`, once and then again after each wait of
/// `every`, until the download `id` has ended. The result is
/// [`download_result`] of how it ended; a read that fails ends the watch
/// with its error.
async fn watch_download<F, Fut>(
    console: Arc<CliConsole>,
    id: &DownloadId,
    every: Duration,
    mut poll: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<QueueSnapshot>>,
{
    let mut watch = QueueWatch::download(console, id);

    loop {
        let snapshot = poll().await?;
        if let Some(ended) = watch.take(&snapshot) {
            return download_result(id, &ended).map_err(anyhow::Error::msg);
        }

        tokio::time::sleep(every).await;
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
