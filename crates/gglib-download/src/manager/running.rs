//! Which download is running.

use gglib_core::download::{DownloadId, DownloadPhase};

use super::DownloadManagerImpl;
use crate::queue::{DownloadQueue, Running};

impl DownloadManagerImpl {
    /// The download that is running, if there is one.
    ///
    /// It is the active job's download, in the phase the job is in. Between
    /// two of its files nothing is active, and it is then the download whose
    /// next file heads the queue, when the tracker has that group open. A
    /// group the tracker has open is not running on that alone: with none of
    /// its files active or pending, there is nothing of it left to run.
    ///
    /// The caller holds the queue lock, and this takes `active` and then the
    /// tracker, so the order is queue → active → tracker.
    pub(super) async fn running(&self, queue: &DownloadQueue) -> Option<Running> {
        if let Some((id, job)) = self.active.lock().await.iter().next() {
            return Some(Running {
                id: id.clone(),
                phase: job.phase,
                file: job.item.shard_info.clone(),
            });
        }
        let (id, group) = queue.head_group()?;
        let is_open = self.shard_tracker.lock().await.is_open(group);
        is_open.then(|| Running {
            id: id.clone(),
            phase: DownloadPhase::Downloading,
            file: queue.next_file_of(id).cloned(),
        })
    }

    /// The id of the running download, with the queue lock held as for
    /// [`Self::running`].
    pub(super) async fn running_id(&self, queue: &DownloadQueue) -> Option<DownloadId> {
        self.running(queue).await.map(|running| running.id)
    }
}

#[cfg(test)]
#[path = "running_tests.rs"]
mod tests;
