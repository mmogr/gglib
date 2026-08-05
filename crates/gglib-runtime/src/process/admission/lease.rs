//! The shared, wakeable handle around [`QueueState`].
//!
//! Everything here is plumbing: a mutex, a notify, and the `Drop` that releases
//! a slot. The rules themselves live in `state.rs`.
//!
//! ## Locking
//!
//! `std::sync::Mutex`, not `tokio`'s — the same call the connection registry
//! makes, for the same reason. Every critical section here is a handful of
//! synchronous map and array operations with no `.await` inside, so it is
//! impossible at the type level to hold the lock across an await point. The one
//! genuinely async part of admission — waiting for a wakeup — happens with the
//! lock released.

use std::sync::{Arc, Mutex, MutexGuard};
use tokio::time::Instant;

use gglib_core::domain::{AdmissionSnapshot, SecondarySlotStatus};
use gglib_core::ports::{AdmissionLease, AdmissionRelease};
use tokio::sync::Notify;

use super::state::{AdmissionDecision, QueueState, Resident, SLOT_COUNT, Ticket};

/// The admission queue: who is resident, who is waiting, and whose turn it is.
///
/// Shared behind an `Arc` by everything that needs to reason about GPU
/// occupancy — see the [module docs](super).
#[derive(Debug, Default)]
pub struct AdmissionQueue {
    state: Mutex<QueueState>,
    /// Woken whenever the state changes in a way that could unblock a waiter:
    /// a lease released, a launch landed, a slot emptied.
    wake: Notify,
}

impl AdmissionQueue {
    /// Create an empty queue with no residents and no waiters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lock the state, recovering from a poisoned mutex rather than panicking.
    ///
    /// A panic inside a critical section here would otherwise wedge every
    /// subsequent request: the queue is on the path of every single one, so
    /// refusing to serve any of them because one unrelated section unwound is
    /// strictly worse than continuing with the state as it was left.
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wake every requester currently waiting, so each can re-evaluate.
    ///
    /// Broadcast rather than targeted: the scheduler's decision depends on
    /// global state (whose turn it is, who has waited longest), so the requester
    /// that should proceed is not knowable from the event that woke it.
    fn notify(&self) {
        self.wake.notify_waiters();
    }

    /// Take a place in line for `model`.
    pub fn enqueue(&self, model: &str) -> Ticket {
        self.lock().enqueue(model, Instant::now())
    }

    /// Give up a place in line without having been granted anything.
    ///
    /// Called when a requester abandons its wait — the deadline elapsed, or the
    /// client disconnected and the future was dropped. Leaving the waiter
    /// behind would let a departed request go on holding the front of the queue
    /// and forcing swaps nobody is waiting for.
    pub fn abandon(&self, ticket: &Ticket) {
        self.lock().forget(ticket);
        self.notify();
    }

    /// Ask what this request should do now.
    ///
    /// `secondary_fits` is consulted only when the second slot is empty and a
    /// co-load is the alternative to a swap. It is a callback rather than a
    /// precomputed flag because free VRAM is a property of the moment: asking
    /// once at enqueue time and acting on it thirty seconds later would be
    /// acting on a reading the primary model has since invalidated.
    pub fn poll(
        &self,
        ticket: &Ticket,
        secondary_fits: &dyn Fn(&str) -> bool,
    ) -> AdmissionDecision {
        self.lock().poll(ticket, Instant::now(), secondary_fits)
    }

    /// A future that resolves the next time the state changes.
    ///
    /// **Must be created and `enable()`d before the `poll` whose result it
    /// backs.** `notify_waiters` only reaches waiters that were already
    /// registered, so a subscription taken out *after* deciding to wait would
    /// miss a wakeup that fired in between and sit out the caller's whole
    /// timeout for nothing.
    ///
    /// Waiting is always raced against a timeout by the caller, because a turn
    /// ageing past its quantum is a purely temporal transition: nothing happens
    /// to fire an event, the clock simply passes.
    pub fn subscribe(&self) -> tokio::sync::futures::Notified<'_> {
        self.wake.notified()
    }

    /// Record that a launch landed, and hand the launching requester its lease.
    ///
    /// The in-flight count is incremented here rather than left to a follow-up
    /// `poll`, so the model cannot be evicted in the window between finishing
    /// its launch and serving the request that paid for it.
    pub fn install(self: &Arc<Self>, slot: usize, resident: Resident) -> AdmissionLease {
        {
            let mut state = self.lock();
            state.install(slot, resident);
            state.retain(slot);
        }
        self.notify();
        AdmissionLease::new(Arc::clone(self) as Arc<dyn AdmissionRelease>, slot)
    }

    /// Record that a launch into `slot` failed, freeing it for another attempt.
    pub fn launch_failed(&self, slot: usize) {
        self.lock().launch_failed(slot);
        self.notify();
    }

    /// Take a lease on a slot already known to be resident.
    ///
    /// Returns `None` when the slot emptied in the meantime, which the caller
    /// treats as "go round again" rather than as an error.
    pub fn lease(self: &Arc<Self>, slot: usize) -> Option<AdmissionLease> {
        self.lock()
            .retain(slot)
            .then(|| AdmissionLease::new(Arc::clone(self) as Arc<dyn AdmissionRelease>, slot))
    }

    /// Wrap an in-flight reference that [`Self::poll`] has *already* counted.
    ///
    /// The counterpart to [`Self::lease`], which counts and wraps in one step.
    /// [`AdmissionDecision::Serve`] increments inside its critical section so
    /// the slot cannot be evicted between the decision and the claim; this is
    /// how the caller takes ownership of that increment. Claiming it is not
    /// optional — a `Serve` that is never claimed leaks a reference and pins
    /// the model resident forever.
    pub fn claim(self: &Arc<Self>, slot: usize) -> AdmissionLease {
        AdmissionLease::new(Arc::clone(self) as Arc<dyn AdmissionRelease>, slot)
    }

    /// Empty a slot on an explicit instruction, returning what was there.
    ///
    /// Unconditional by design: this is a user or supervisor decision (a stop
    /// request, a degraded-model recycle), not a scheduling one, so it is not
    /// subject to the fairness rules.
    pub fn evict(&self, slot: usize) -> Option<Resident> {
        let previous = self.lock().evict(slot);
        self.notify();
        previous
    }

    /// Empty every slot, returning what was there.
    pub fn evict_all(&self) -> Vec<Resident> {
        let evicted = {
            let mut state = self.lock();
            (0..SLOT_COUNT)
                .filter_map(|slot| state.evict(slot))
                .collect()
        };
        self.notify();
        evicted
    }

    /// The primary slot's resident.
    pub fn primary(&self) -> Option<Resident> {
        self.lock().primary().cloned()
    }

    /// Every resident, primary first.
    pub fn residents(&self) -> Vec<(usize, Resident)> {
        self.lock()
            .residents()
            .map(|(slot, r)| (slot, r.clone()))
            .collect()
    }

    /// The slot holding or loading `model`.
    pub fn slot_of(&self, model: &str) -> Option<usize> {
        self.lock().slot_of(model)
    }

    /// The resident in `slot`, if any.
    pub fn slot(&self, slot: usize) -> Option<Resident> {
        self.lock().slot(slot).cloned()
    }

    /// Whether any slot is mid-launch.
    pub fn is_loading(&self) -> bool {
        self.lock().is_loading()
    }

    /// Record the most recent second-slot verdict, so an idle secondary can
    /// explain itself on the dashboard.
    ///
    /// Skips the write when nothing changed, keeping the steady state — every
    /// request reaching the same verdict — off the publisher's back.
    pub fn record_secondary_status(&self, status: SecondarySlotStatus) {
        let mut state = self.lock();
        if state.secondary_slot != status {
            state.secondary_slot = status;
        }
    }

    /// Project the queue for the dashboard.
    pub fn snapshot(&self) -> AdmissionSnapshot {
        self.lock().snapshot(Instant::now())
    }
}

impl AdmissionRelease for AdmissionQueue {
    fn release(&self, slot: usize) {
        self.lock().release(slot);
        // The release is the event the scheduler has been waiting for: a slot
        // reaching zero in-flight is the only thing that makes a swap legal.
        self.notify();
    }
}
