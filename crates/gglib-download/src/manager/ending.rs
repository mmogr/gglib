//! How a download ends.
//!
//! A download is every file of one model, and it ends as a whole: when its
//! last file is registered, when one of its files fails, or when the user
//! stops it. [`DownloadManagerImpl::end_download`] is the one place that
//! happens, whichever of those it was and whatever the download was doing.

use gglib_core::download::{
    CompletionKind, DownloadError, DownloadEvent, DownloadId, DownloadOutcome, FinishedDownload,
};

use super::{ActiveJob, DownloadManagerImpl};
use crate::queue::DownloadQueue;

impl DownloadManagerImpl {
    /// End the download `id` with `outcome`, and answer its finished entry.
    /// `None` when nothing of it is left: no file pending, and no `job`.
    ///
    /// Its files still pending leave the queue, its group is closed in the
    /// tracker so a file landing late cannot open it again, its meter is
    /// dropped, and its outcome joins the finished list and the run's
    /// summary. `job` is its file that was being fetched, already taken out
    /// of `active` by the caller.
    ///
    /// The caller holds the queue's write guard and passes the queue, so
    /// all of that is one change to a reader: no snapshot shows a download
    /// gone with no outcome, or ended with a file still waiting. Locks taken
    /// under it: the tracker, the meters, then the run's summary.
    async fn end_download(
        &self,
        queue: &mut DownloadQueue,
        id: &DownloadId,
        job: Option<ActiveJob>,
        outcome: DownloadOutcome,
    ) -> Option<FinishedDownload> {
        let mut pending = queue.take_pending(id);
        let item = match job {
            Some(job) => job.item,
            None if pending.is_empty() => return None,
            None => pending.swap_remove(0),
        };

        if let Some(group) = &item.group_id {
            self.shard_tracker.lock().await.close(group);
        }
        self.meters().remove(id);
        let kind = match &outcome {
            DownloadOutcome::Completed { .. } => CompletionKind::Downloaded,
            DownloadOutcome::Failed { .. } => CompletionKind::Failed,
            DownloadOutcome::Cancelled => CompletionKind::Cancelled,
        };
        self.record_completion_in_run(&item, kind).await;
        Some(queue.record_outcome(id, outcome))
    }

    /// Say that downloads ended, and publish the queue without them. The
    /// caller has let go of the queue.
    async fn announce(&self, ended: &[FinishedDownload]) {
        for ended in ended {
            tracing::info!(id = %ended.id, outcome = ?ended.outcome, "Download ended");
            self.emit(DownloadEvent::ended(ended));
        }
        self.publish().await;
    }

    /// Take the file the worker has returned from out of `active`, and end
    /// its download when `outcome` says it has ended.
    ///
    /// With no outcome the download has files still to come, and goes on,
    /// unless it was cancelled while this file was being put away: then it
    /// ends here, cancelled, and not between two files with nobody to stop
    /// it. The file leaves `active` and the outcome is recorded under one
    /// queue guard.
    pub(super) async fn settle(&self, id: &DownloadId, outcome: Option<DownloadOutcome>) {
        let mut queue = self.queue.write().await;
        let job = self.active.lock().await.remove(id);
        let cancelled = job.as_ref().is_some_and(|job| job.cancel.is_cancelled());
        let outcome = outcome.or_else(|| cancelled.then_some(DownloadOutcome::Cancelled));
        let ended = match outcome {
            Some(outcome) => self.end_download(&mut queue, id, job, outcome).await,
            None => None,
        };
        drop(queue);

        self.announce(ended.as_slice()).await;
    }

    /// Stop the download `id` at the user's word. Answers whether it was in
    /// flight.
    ///
    /// A download with a file in `active` has its token cancelled, and this
    /// answers true. While the file is being fetched that stops the worker,
    /// and the worker's return ends the download, cancelled whatever the
    /// worker answers. Once `finalize_job` has read the token for the
    /// download's last file, or for a file that failed, the cancel is too
    /// late: the download ends completed or failed. One waiting, or between
    /// two of its files, ends here.
    pub(super) async fn stop_download(&self, id: &DownloadId) -> bool {
        let mut queue = self.queue.write().await;
        if let Some(job) = self.active.lock().await.get(id) {
            job.cancel.cancel();
            tracing::info!(id = %id, "Cancelling the download being fetched");
            return true;
        }
        let ended = self
            .end_download(&mut queue, id, None, DownloadOutcome::Cancelled)
            .await;
        drop(queue);

        if ended.is_none() {
            return false;
        }
        self.announce(ended.as_slice()).await;
        true
    }

    /// Stop every download: the one being fetched as [`Self::stop_download`]
    /// does, and each one waiting or between two files here, with its own
    /// cancelled outcome.
    pub(super) async fn stop_all(&self) {
        let mut queue = self.queue.write().await;
        let mut fetching = Vec::new();
        for (id, job) in self.active.lock().await.iter() {
            job.cancel.cancel();
            fetching.push(id.clone());
        }
        let mut ended = Vec::new();
        for id in queue.pending_ids() {
            if !fetching.contains(&id) {
                let outcome = DownloadOutcome::Cancelled;
                ended.extend(self.end_download(&mut queue, &id, None, outcome).await);
            }
        }
        drop(queue);

        self.announce(&ended).await;
    }

    /// Take `id` off the queue: stop it when it is in flight, and otherwise
    /// drop its entry from the finished list.
    pub(super) async fn take_off(&self, id: &DownloadId) -> Result<(), DownloadError> {
        if self.stop_download(id).await {
            return Ok(());
        }
        if !self.queue.write().await.forget_outcome(id) {
            return Err(DownloadError::not_in_queue(id.to_string()));
        }
        tracing::info!(id = %id, "Dropped a finished download's entry");
        self.publish().await;
        Ok(())
    }
}
