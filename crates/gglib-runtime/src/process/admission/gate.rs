//! The generation gate's plumbing: waiting for a turn, ending one, and
//! retiring a render whose process had to be killed.
//!
//! The rules are in `state_gate.rs`. As everywhere in this module, every
//! critical section takes plain data in and hands plain data out; the
//! observer, and the kill a retirement awaits, run with the lock released.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ports::{
    AdmissionLease, GateError, GateRelease, GateWaitObserver, GenerationGate, GenerationTurn,
    TurnKind,
};
use tokio::time::Instant;

use super::lease::AdmissionQueue;
use super::state::{GateTicket, GateVerdict, Resident};

/// How often a waiter re-asks with no wakeup, so the stall deadline is
/// noticed when nothing at all happens. The same tick residency waits on.
const GATE_POLL_TICK: Duration = Duration::from_millis(250);

/// What a render is told when its image model was stopped under it.
pub const RENDER_STOPPED: &str = "the image model was stopped before the picture was finished";

/// The admission queue seen as a [`GenerationGate`].
#[derive(Debug)]
struct QueueGate(Arc<AdmissionQueue>);

#[async_trait]
impl GenerationGate for QueueGate {
    async fn llm_turn(
        &self,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        self.0.wait_for_turn(TurnKind::Llm, None, observer).await
    }

    async fn render_turn(
        &self,
        lease: AdmissionLease,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        self.0
            .wait_for_turn(TurnKind::Render, Some(lease), observer)
            .await
    }
}

/// Leaves the gate's line when the wait ends, however it ends: a granted or
/// stalled waiter is already gone, a dropped future is removed here.
struct QueuedTurn<'a> {
    queue: &'a AdmissionQueue,
    ticket: GateTicket,
}

impl Drop for QueuedTurn<'_> {
    fn drop(&mut self) {
        self.queue.lock().gate_forget(&self.ticket);
        self.queue.notify();
    }
}

impl AdmissionQueue {
    /// This queue's generation gate.
    #[must_use]
    pub fn generation_gate(self: &Arc<Self>) -> Arc<dyn GenerationGate> {
        Arc::new(QueueGate(Arc::clone(self)))
    }

    /// Wait for a turn of `kind`, holding `lease` (a render's) meanwhile.
    async fn wait_for_turn(
        self: &Arc<Self>,
        kind: TurnKind,
        lease: Option<AdmissionLease>,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError> {
        let queued = QueuedTurn {
            queue: self,
            ticket: self.lock().gate_enqueue(
                kind,
                lease.as_ref().map(AdmissionLease::slot),
                Instant::now(),
            ),
        };
        let mut told = None;
        loop {
            // Subscribed before the poll, as residency's wait is, so a wakeup
            // between the answer and the wait is not lost.
            let changed = self.subscribe();
            tokio::pin!(changed);
            changed.as_mut().enable();

            let verdict = self.lock().gate_poll(&queued.ticket, Instant::now());
            match verdict {
                GateVerdict::Granted { id } => {
                    let owner = Arc::clone(self) as Arc<dyn GateRelease>;
                    return Ok(GenerationTurn::new(owner, id, kind, lease));
                }
                GateVerdict::Stalled(waited) => return Err(GateError::Stalled(waited)),
                // The lease drops with this, on a slot the Stop has not
                // emptied yet: the Stop waits for exactly that.
                GateVerdict::Stopped => {
                    return Err(GateError::Unavailable(RENDER_STOPPED.to_owned()));
                }
                GateVerdict::Wait(wait) => {
                    // Outside the lock, and only when there is news.
                    if let (Some(observer), Some(wait)) = (&observer, wait)
                        && told != Some(wait)
                    {
                        observer.waiting(wait);
                        told = Some(wait);
                    }
                    tokio::select! {
                        () = changed => {}
                        () = tokio::time::sleep(GATE_POLL_TICK) => {}
                    }
                }
            }
        }
    }

    /// Ask every render on model `model_id` to stop: the one drawing with
    /// it and any waiting in line with a lease on it. Whether there was one.
    ///
    /// A Stop must not empty a slot a render's lease pins: the lease,
    /// released later by slot, would take a request from whatever was
    /// launched there in between. So the Stop asks, and waits until this
    /// answers `false`: a waiting render leaves the line and drops its
    /// lease, and the driver of one that is drawing retires it
    /// ([`Self::retire_render`]) at its next look at the job.
    pub fn ask_render_stop(&self, model_id: u32) -> bool {
        let (asked, news) = self.lock().gate_ask_render_stop(model_id);
        // Only for a render not asked before: a Stop that waits here asks
        // again at every wakeup, and must not be the one that wakes it.
        if news {
            self.notify();
        }
        asked
    }

    /// Whether a Stop was asked of model `model_id` while the render that
    /// holds the GPU draws with it: what its driver reads to retire it.
    #[must_use]
    pub fn render_stop_asked(&self, model_id: u32) -> bool {
        self.lock().gate_render_stop_asked(model_id)
    }

    /// Retire a render whose `sd-server` had to be killed (a stall, the job
    /// deadline, or a person's Stop), in the one order that cannot hurt a
    /// newcomer.
    ///
    /// 1. `kill` runs to completion first, with no lock held.
    /// 2. Under one lock, the render's slot is emptied, which settles its
    ///    lease count, only while the slot still holds `model_id`
    ///    (`QueueState::retire`).
    /// 3. The lease is disarmed, since its count is settled, and the turn
    ///    ends.
    ///
    /// Releasing the lease by slot after the slot was emptied would take an
    /// in-flight request from whatever model was launched there in between;
    /// this never does. Returns the resident it emptied, if it did.
    pub async fn retire_render(
        &self,
        mut turn: GenerationTurn,
        model_id: u32,
        kill: impl Future<Output = ()>,
    ) -> Option<Resident> {
        kill.await;
        let lease = turn.take_lease();
        let retired = lease
            .as_ref()
            .and_then(|lease| self.lock().retire(lease.slot(), model_id));
        self.notify();
        if let Some(lease) = lease {
            lease.disarm();
        }
        drop(turn);
        retired
    }
}

impl GateRelease for AdmissionQueue {
    fn progress(&self, id: u64, step: u32, total: u32) {
        // Kept for observers; the lease's own progress, which follows, is what
        // wakes them and counts as queue progress.
        self.lock().gate_progress(id, step, total);
    }

    fn end(&self, id: u64) {
        self.lock().gate_end(id);
        self.notify();
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
