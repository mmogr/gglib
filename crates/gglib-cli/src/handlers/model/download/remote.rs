//! Progress monitor for downloads running on the gglib daemon.
//!
//! Presentation only: the queue snapshot arrives from
//! [`DaemonHandle::download_queue`], and this module hands it to the download
//! board. The download itself belongs to the daemon — Ctrl-C here (or a
//! closed terminal) detaches the monitor and the download keeps going, which
//! is the point of daemon ownership.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use gglib_core::download::{DownloadId, QueueSnapshot};
use gglib_core::services::{RepairStarted, missing_after_repair};

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
    let poll = || poll(handle);
    queue_then_watch(console, queue, POLL_INTERVAL, poll, detached()).await
}

/// Wait for `repair`, the request that has the daemon delete a model's
/// unhealthy files and queue the download that fetches them again, and watch
/// that download as [`monitor`] watches one it queued.
///
/// A repair the daemon refuses ends here with the daemon's words. A download
/// that does not complete is an error that also names the files still
/// missing from `folder`, the model's, and the command that fetches them.
pub(in crate::handlers::model) async fn monitor_repair<R>(
    handle: &DaemonHandle,
    console: Arc<CliConsole>,
    folder: &Path,
    repair: R,
) -> Result<()>
where
    R: Future<Output = Result<RepairStarted>>,
{
    let poll = || poll(handle);
    repair_then_watch(console, folder, repair, POLL_INTERVAL, poll, detached()).await
}

/// The daemon's download queue, as one read of a watch.
async fn poll(handle: &DaemonHandle) -> Result<QueueSnapshot> {
    handle
        .download_queue()
        .await
        .context("polling the daemon download queue")
}

/// Completes at Ctrl-C, having said that the download goes on without this
/// command.
async fn detached() {
    let _ = tokio::signal::ctrl_c().await;
    eprintln!();
    eprintln!("  Detached \u{2014} the download continues on the gglib daemon.");
    eprintln!("  Re-attach anytime with `gglib model download <id>` or the dashboard.");
}

/// Wait for `repair` to answer its download and the files that download is
/// to bring back, say so, and watch the download with [`queue_then_watch`].
///
/// A watch that ends in an error has not brought every file back: the error
/// goes on to name those of them not in `folder`, with
/// [`missing_after_repair`]'s words.
async fn repair_then_watch<R, F, Fut, D>(
    console: Arc<CliConsole>,
    folder: &Path,
    repair: R,
    every: Duration,
    poll: F,
    detached: D,
) -> Result<()>
where
    R: Future<Output = Result<RepairStarted>>,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<QueueSnapshot>>,
    D: Future<Output = ()>,
{
    let started = repair.await?;
    let id = DownloadId::from(started.id.as_str());
    console.println(&format!(
        "Fetching again as {id}: {}",
        started.files.join(", ")
    ));

    let queued = std::future::ready(Ok(id.clone()));
    let Err(failed) = queue_then_watch(console, queued, every, poll, detached).await else {
        return Ok(());
    };
    let missing: Vec<String> = started
        .files
        .into_iter()
        .filter(|file| !folder.join(file).exists())
        .collect();
    if missing.is_empty() {
        return Err(failed);
    }
    let missing = missing_after_repair(&id, &missing);
    Err(anyhow::anyhow!("{failed:#}\n{missing}"))
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

#[cfg(test)]
#[path = "remote_repair_tests.rs"]
mod repair_tests;
