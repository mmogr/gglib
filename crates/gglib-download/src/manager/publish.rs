//! Building the queue snapshot and sending it out.
//!
//! There is one builder. The REST route and the event stream are both
//! served what [`DownloadManagerImpl::build_snapshot`] makes, and every
//! snapshot is numbered under one mutex, so no two surfaces can be shown
//! different queues and no snapshot is sent out of order.

use std::sync::{Arc, MutexGuard, PoisonError};
use std::time::Instant;

use tokio::sync::watch;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use gglib_core::download::{DownloadEvent, DownloadId, DownloadPhase, QueueSnapshot};

use super::meter::GroupMeter;
use super::{DownloadManagerImpl, PROGRESS_TICK, ProgressUpdate};

impl DownloadManagerImpl {
    /// The meters, one per download that has started. A std mutex: it is
    /// taken last, for a moment, and never held across an await.
    pub(super) fn meters(
        &self,
    ) -> MutexGuard<'_, std::collections::HashMap<DownloadId, GroupMeter>> {
        self.meters.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Build the next snapshot and send it to every subscriber.
    ///
    /// The publish mutex is held from the build to the send, so snapshots
    /// leave in the order of their revisions. The caller must hold none of
    /// the queue, `active`, the tracker or the meters: this takes them.
    pub(super) async fn publish(&self) {
        let mut revision = self.publish.lock().await;
        let snapshot = self.build_snapshot(&mut revision).await;

        // Drained is nothing running, nothing waiting and no group with
        // files still to come.
        let has_open_groups = self.shard_tracker.lock().await.has_open_groups();
        let is_drained = snapshot.is_idle() && !has_open_groups;
        tracing::debug!(
            target: "gglib.download",
            revision = snapshot.revision,
            active = snapshot.active.is_some(),
            waiting = snapshot.waiting.len(),
            has_open_groups,
            is_drained,
            "Publishing queue snapshot"
        );
        self.handle_drain_transitions(is_drained).await;

        self.emit(DownloadEvent::queue_snapshot(snapshot));

        // Held until the snapshot is sent: that is what orders them.
        drop(revision);
    }

    /// The queue as it is now, numbered with the next revision.
    ///
    /// The caller holds the publish mutex and passes its counter. The queue
    /// is held for the whole read, so a file cannot leave `pending` for
    /// `active` between the two being looked at. Lock order: queue → active
    /// → tracker → meters.
    pub(super) async fn build_snapshot(&self, revision: &mut u64) -> QueueSnapshot {
        let queue = self.queue.read().await;
        let running = self.running(&queue).await;
        let reading = running
            .as_ref()
            .and_then(|running| self.meters().get(&running.id).map(GroupMeter::reading));

        *revision += 1;
        queue.snapshot(*revision, running.as_ref(), reading.as_ref())
    }

    /// Move the active download to `phase` and publish it, so the phase is
    /// on the next snapshot a client reads and not only on a passing event.
    pub(super) async fn set_phase(&self, id: &DownloadId, phase: DownloadPhase) {
        if let Some(job) = self.active.lock().await.get_mut(id) {
            job.phase = phase;
        }
        self.publish().await;
    }

    /// Feed the latest reading of the file in flight to its download's
    /// meter.
    pub(super) fn observe(&self, id: &DownloadId, update: &ProgressUpdate, now: Instant) {
        if let Some(meter) = self.meters().get_mut(id) {
            meter.observe(update.progress, update.notice.as_deref(), now);
        }
    }

    /// Sample a file's progress on a fixed tick while it is fetched: feed
    /// the meter, then publish.
    ///
    /// Every tick is fed to the meter, including those where no byte moved.
    /// Those idle samples are what let a stalled transfer decay toward zero
    /// instead of freezing the speed shown, and a snapshot goes out on every
    /// tick for the same reason.
    ///
    /// It ends on `finished`, which the run loop signals when the worker
    /// returns and then waits on. The last reading is taken on the way out,
    /// so the meter has the file's final count before the file is finalized;
    /// the snapshot that shows it is the one finalizing sends. On `cancel` it
    /// stops at once.
    pub(super) async fn run_meter(
        self: Arc<Self>,
        id: DownloadId,
        rx: watch::Receiver<ProgressUpdate>,
        cancel: CancellationToken,
        finished: CancellationToken,
    ) {
        let mut tick = interval(PROGRESS_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;

                () = cancel.cancelled() => break,

                () = finished.cancelled() => {
                    let last = rx.borrow().clone();
                    self.observe(&id, &last, Instant::now());
                    break;
                }

                _ = tick.tick() => {
                    let current = rx.borrow().clone();
                    self.observe(&id, &current, Instant::now());
                    self.publish().await;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod outcome_tests;
#[cfg(test)]
#[path = "publish_when_tests.rs"]
mod publish_when_tests;
#[cfg(test)]
#[path = "requeue_tests.rs"]
mod requeue_tests;
#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
