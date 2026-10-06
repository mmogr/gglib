//! Which download is running.

use gglib_core::download::{DownloadId, QueuedDownload};

use super::DownloadManagerImpl;
use crate::queue::DownloadQueue;

impl DownloadManagerImpl {
    /// The download that is running, if there is one.
    ///
    /// It is the active job's download. Between two of its files nothing is
    /// active, and it is then the download whose next file heads the queue,
    /// when the tracker has that group open. A group the tracker has open is
    /// not running on that alone: with none of its files active or pending,
    /// there is nothing of it left to run.
    ///
    /// The caller holds the queue lock, and this takes `active` and then the
    /// tracker, so the order is queue → active → tracker.
    pub(super) async fn running_id(&self, queue: &DownloadQueue) -> Option<DownloadId> {
        if let Some(id) = self.active.lock().await.keys().next() {
            return Some(id.clone());
        }
        let (id, group) = queue.head_group()?;
        let is_open = self.shard_tracker.lock().await.is_open(group);
        is_open.then(|| id.clone())
    }

    /// The running download's row while nothing of it is being fetched, with
    /// the queue lock held as for [`Self::running_id`].
    pub(super) async fn between_files_row(&self, queue: &DownloadQueue) -> Option<QueuedDownload> {
        let running = self.running_id(queue).await?;
        queue.between_files_row(&running)
    }
}

#[cfg(test)]
#[path = "running_tests.rs"]
mod tests;
