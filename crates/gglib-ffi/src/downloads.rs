//! Starting a download, and pausing and resuming every one.
//!
//! Every download goes through the library core's download manager, which
//! fetches into the library's models folder, keeps an interrupted file as a
//! `.part` that the next attempt resumes with a ranged request, checks the
//! file against the digest the Hub gives, and adds the model to the library
//! once every file is in.
//!
//! The manager has no pause. Pausing cancels each download in the queue,
//! which keeps its `.part`, and remembers it; resuming queues each again, in
//! the order they stood, and each carries on from its `.part`. A transfer
//! stops a moment after its cancel, and the manager attaches a download
//! queued under the id of one still running to it, so pausing waits for
//! each to stop.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::download::DownloadId;
use gglib_core::ports::{DownloadManagerPort, HfClientPort};

use crate::download_feed::DownloadFeed;
use crate::download_watch::DownloadWatch;
use crate::errors::GglibError;

/// Queue quantization `quantization` of `repo`, and watch it.
///
/// The Hub is asked for the quantization's files first, so a repository or
/// quantization that does not exist, or that wants a token, is refused with
/// the Hub's own error before anything is queued. A download already
/// waiting or running is watched rather than queued twice.
pub(crate) async fn start(
    hub: &dyn HfClientPort,
    downloads: Arc<dyn DownloadManagerPort>,
    feed: Arc<DownloadFeed>,
    repo: String,
    quantization: String,
) -> Result<DownloadWatch, GglibError> {
    if hub
        .get_quantization_files(&repo, &quantization)
        .await?
        .is_empty()
    {
        return Err(GglibError::NotFound);
    }
    let since = feed.count();
    let id = Arc::clone(&downloads)
        .queue_smart(repo, Some(quantization))
        .await?;
    // Asked for again, it is no longer waiting on a resume.
    feed.unpause(&id.to_string());
    Ok(DownloadWatch::new(id, since, feed, downloads))
}

/// How long [`pause_all`] waits for the downloads it cancelled to stop: as
/// long as the download manager's own `cancel_all` waits for its transfers.
const WIND_DOWN: Duration = Duration::from_secs(5);

/// Cancel every download waiting or running, keeping what each fetched,
/// remember them as paused, and wait up to [`WIND_DOWN`] for each to stop.
pub(crate) async fn pause_all(downloads: &dyn DownloadManagerPort, feed: &DownloadFeed) {
    let Ok(queue) = downloads.get_queue_snapshot().await else {
        return;
    };
    let rows: Vec<_> = queue.rows().cloned().collect();
    // Noted first, so the cancellation each ends with reads as a pause.
    feed.pause(&rows);
    let mut ids = Vec::with_capacity(rows.len());
    for row in rows {
        let id = DownloadId::new(row.model_id, row.quantization);
        // One that ended in the meantime has nothing left to cancel.
        let _ = downloads.cancel_download(&id).await;
        ids.push(row.id);
    }
    // One still running past the wait stays paused at a resume until it
    // has stopped (`DownloadFeed::take_paused`).
    let _ = tokio::time::timeout(WIND_DOWN, feed.until_stopped(&ids)).await;
}

/// Queue every paused download that has stopped again, in the order they
/// stood. One that cannot be queued, for want of a network among other
/// things, stands failed with the reason, and its watch ends.
pub(crate) async fn resume_all(downloads: &Arc<dyn DownloadManagerPort>, feed: &DownloadFeed) {
    for paused in feed.take_paused() {
        if let Err(error) = Arc::clone(downloads)
            .queue_smart(paused.repo, paused.quantization)
            .await
        {
            feed.fail(&paused.id, error.to_string());
        }
    }
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
