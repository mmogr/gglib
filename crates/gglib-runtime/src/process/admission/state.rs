//! The admission queue's state and its scheduling decision.
//!
//! Split from [`AdmissionQueue`](super::AdmissionQueue) so the decision logic
//! is reachable without the `Arc`/`Notify` plumbing around it: every rule in
//! here is a synchronous function over plain data, which is what makes the
//! fairness bounds testable at all. The wiring — locking, waking, the lease's
//! `Drop` — lives next door in `lease.rs`.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;
// Tokio's clock rather than `std`'s: identical in a normal runtime, but it is
// the one `tokio::time::pause()` advances, which is what lets a test drive a
// whole drain quantum without sleeping through it.
use tokio::time::Instant;

use gglib_core::domain::{
    AdmissionSnapshot, CacheRamHealth, LaunchNarration, ModelSamplingDefaults, QueuedModelSnapshot,
    ResidentSlotSnapshot, SecondarySlotDecision, SecondarySlotStatus,
};

use crate::command::SERVER_PARALLEL;

/// How many models may be resident in VRAM at once.
///
/// Two: one primary, plus room for a small auxiliary model that would otherwise
/// spend its life being swapped in and out. Three or more is deliberately not
/// offered — the memory arithmetic stops being trustworthy well before the
/// scheduling does, and a card with room for three useful models is not the
/// hardware this project targets.
pub const SLOT_COUNT: usize = 2;

/// The slot chat traffic and the llama.cpp `/slots` poller follow.
pub const PRIMARY_SLOT: usize = 0;

/// How long a model keeps the GPU once it has won a turn, while another model
/// is waiting.
///
/// The value trades swap amortisation against rival latency. A swap costs
/// roughly 5–15 s (process teardown, weight load, health check), so a quantum
/// materially shorter than that would spend more time swapping than serving.
/// Twenty seconds drains a healthy burst while keeping the rival's wait inside
/// what an agentic client will sit through.
pub const DRAIN_QUANTUM: Duration = Duration::from_secs(20);

/// How long one model launch may take before it is abandoned.
///
/// The health check itself waits up to 120 s for llama-server to come up; the
/// remainder covers model resolution, the displaced process's shutdown, and the
/// spawn.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(150);

/// How long a request may wait **with no queue progress** before giving up
/// with a 503.
///
/// A *stall* deadline, not a wall clock. It used to run from enqueue
/// regardless of what the queue was doing, which made two very different
/// situations expire identically: a genuinely wedged queue, and a first
/// request on a cold daemon waiting out a model load that was proceeding
/// normally — the second being a real failure observed three times in one
/// day of benchmarking (a retry immediately succeeded each time, because the
/// load the timeout had abandoned was in fact landing).
///
/// So the clock now measures time since the queue last did anything on
/// anyone's behalf, and two things hold it back:
///
/// - **A launch in flight pauses every waiter's clock.** A loading slot is
///   the opposite of a stall, and it is bounded on its own: [`LAUNCH_TIMEOUT`]
///   abandons an overrunning launch and frees the slot, at which point the
///   clock runs again. Waiting out a load can therefore extend a wait by at
///   most one launch budget at a time, never indefinitely.
/// - **Progress resets the clock.** A lease released (a generation finished),
///   a launch landing or failing, a slot evicted — each proves the queue is
///   moving, so a waiter behind it is queued, not stuck.
///
/// What still expires is exactly what this deadline always existed for.
/// [`DRAIN_QUANTUM`] bounds how long a *turn* lasts, but a turn cannot end
/// while the outgoing model still has requests in flight — no swap may preempt
/// a live generation. A single generation is indivisible, so the rival behind
/// it has no bound but this one: an in-flight count that stays pinned with no
/// release for this long is a hog or a wedge, and the waiter behind it gets
/// its 503.
///
/// It used to cover a great deal more. A model under *overlapping* load could
/// hold its slot indefinitely, because nothing capped how many requests were in
/// flight at once and a client that always kept one outstanding meant the count
/// never reached zero. `SERVER_PARALLEL` and `owes_slot_to_rival` close that
/// off, so reaching this deadline is now the exception it was meant to be
/// rather than the ordinary outcome of two clients sharing an endpoint.
pub const ADMISSION_DEADLINE: Duration = Duration::from_secs(180);

/// A model loaded in VRAM, and everything the fast path needs to know about it.
///
/// This carries what `CurrentModelState` used to, plus the in-flight count that
/// makes eviction decisions safe. The launch metadata is cached here for the
/// same reason it always was: the resolutions only exist at spawn, so a later
/// request has no way to recover them.
#[derive(Debug, Clone)]
pub struct Resident {
    /// Database ID of the resident model.
    pub model_id: u32,
    /// Model name, matched exactly against the requested name.
    pub model_name: String,
    /// Context size this instance launched with.
    pub context_size: u64,
    /// Port its llama-server is listening on.
    pub port: u16,
    /// Path to the model file on disk.
    pub model_path: PathBuf,
    /// Whether disk slot restore can resume this model.
    pub slot_restore_supported: bool,
    /// What this model's GGUF declares about sampler defaults.
    ///
    /// Not `Option`: a resident always came from a `ModelLaunchSpec`, so a
    /// GGUF was always read. "Read and declares nothing" is
    /// `ModelSamplingDefaults::default()`, which is the ordinary case.
    pub model_sampling: ModelSamplingDefaults,
    /// Health of the `--cache-ram` budget this instance launched with.
    pub cache_ram_health: CacheRamHealth,
    /// What this instance's launch decided.
    pub narration: Option<LaunchNarration>,
    /// On-disk size of this model's weights, all shards.
    ///
    /// Carried so a second launch can budget against what is *left* rather than
    /// against the whole machine — see
    /// [`ram_available_for`](crate::process::residency::ram_available_for).
    pub weights_bytes: u64,
    /// Requests currently being served by this model.
    ///
    /// The eviction guard. A slot with `inflight > 0` is never unloaded, so a
    /// swap can never cut off a live generation.
    ///
    /// Bounded by `SERVER_PARALLEL`: admitting more than the instance can
    /// actually start would not serve anyone sooner, it would only move the
    /// queue inside llama-server and keep this count off zero the whole time it
    /// sat there.
    pub inflight: u32,
    /// When this model finished loading.
    pub resident_since: Instant,
}

/// What a slot is doing.
#[derive(Debug, Clone, Default)]
pub enum SlotState {
    /// Nothing loaded, nothing loading.
    #[default]
    Empty,
    /// A launch is in progress for this model. Exactly one requester drives it;
    /// everyone else waits and takes the fast path once it lands.
    Loading {
        /// The model being launched.
        model: String,
    },
    /// A model is loaded and serving.
    Resident(Box<Resident>),
}

impl SlotState {
    /// The resident model, if this slot has one loaded.
    pub const fn resident(&self) -> Option<&Resident> {
        match self {
            Self::Resident(r) => Some(r),
            Self::Empty | Self::Loading { .. } => None,
        }
    }

    /// The model this slot holds or is loading, if any.
    fn model(&self) -> Option<&str> {
        match self {
            Self::Empty => None,
            Self::Loading { model } => Some(model),
            Self::Resident(r) => Some(&r.model_name),
        }
    }

    /// Whether this slot could be handed to a different model right now.
    ///
    /// A slot mid-launch is never evictable — the launch would be wasted and
    /// the process left orphaned. A resident slot is evictable only when
    /// nothing is being served from it.
    fn is_evictable(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Loading { .. } => false,
            Self::Resident(r) => r.inflight == 0,
        }
    }
}

/// One request waiting for a model.
#[derive(Debug, Clone)]
struct Waiter {
    /// Global arrival order, so FIFO holds across models rather than only
    /// within one.
    seq: u64,
    /// When it started waiting, reported on the dashboard as the queue depth's
    /// age so a user can see a backlog forming.
    enqueued_at: Instant,
    /// When this waiter last saw the queue do anything — the reference point
    /// [`ADMISSION_DEADLINE`] measures from. Reset on every progress event and
    /// held back while a launch is in flight; `enqueued_at` stays untouched so
    /// the dashboard keeps reporting the true wait.
    stalled_since: Instant,
    /// The [`QueueState::progress_epoch`] this waiter has already accounted
    /// for. A counter rather than re-deriving from state, because progress is
    /// a *transition* — a release followed by an immediate re-acquire leaves
    /// the state looking identical, and the waiter behind it would otherwise
    /// never learn the queue had moved.
    seen_epoch: u64,
}

/// A registered place in the queue.
///
/// Held by the requester for the whole of its `admit` call. Dropping it removes
/// the waiter, which is what stops a disconnected client from holding a model's
/// turn open — see [`QueueState::forget`].
#[derive(Debug)]
pub struct Ticket {
    /// The model this request wants.
    pub(super) model: String,
    /// Its place in the global arrival order.
    pub(super) seq: u64,
    /// When it was created, for [`ADMISSION_DEADLINE`].
    pub(super) created_at: Instant,
}

/// Which model currently owns the right to keep the GPU.
#[derive(Debug, Clone)]
struct Turn {
    model: String,
    started_at: Instant,
}

/// What a requester should do next.
#[derive(Debug, PartialEq, Eq)]
pub enum AdmissionDecision {
    /// The model is resident in this slot and the in-flight count has already
    /// been incremented on the requester's behalf. It must now be released
    /// exactly once, via the lease.
    Serve {
        /// Which slot holds it.
        slot: usize,
    },
    /// The requester must drive a launch into this slot. The slot is already
    /// marked as loading, so no other requester will be given the same job.
    Launch {
        /// The slot to launch into.
        slot: usize,
        /// Model id to stop first, when the slot is currently occupied.
        evict: Option<u32>,
    },
    /// Nothing to do yet — wait for a wakeup and ask again.
    Wait,
    /// The request outlasted [`ADMISSION_DEADLINE`].
    Expired,
}

/// Running totals, reported on the dashboard.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Stats {
    pub(super) total_queued: u64,
    pub(super) total_swaps: u64,
}

/// Everything the scheduler reasons over.
#[derive(Debug)]
pub(super) struct QueueState {
    slots: [SlotState; SLOT_COUNT],
    waiting: HashMap<String, VecDeque<Waiter>>,
    next_seq: u64,
    turn: Option<Turn>,
    pub(super) stats: Stats,
    /// The most recent second-slot verdict, kept so the dashboard can explain
    /// an idle secondary rather than just showing it empty.
    pub(super) secondary_slot: SecondarySlotStatus,
    /// Bumped on every event that proves the queue is moving: a launch
    /// starting, landing or failing, a lease released, a slot evicted.
    /// Waiters compare against it to tell a queue that is working through
    /// its backlog from one that has stalled — see [`ADMISSION_DEADLINE`].
    progress_epoch: u64,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            slots: [SlotState::Empty, SlotState::Empty],
            waiting: HashMap::new(),
            next_seq: 0,
            turn: None,
            stats: Stats::default(),
            secondary_slot: SecondarySlotStatus::default(),
            progress_epoch: 0,
        }
    }
}

impl QueueState {
    /// Register a request and return its place in line.
    pub(super) fn enqueue(&mut self, model: &str, now: Instant) -> Ticket {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.stats.total_queued += 1;
        self.waiting
            .entry(model.to_owned())
            .or_default()
            .push_back(Waiter {
                seq,
                enqueued_at: now,
                stalled_since: now,
                seen_epoch: self.progress_epoch,
            });
        Ticket {
            model: model.to_owned(),
            seq,
            created_at: now,
        }
    }

    /// Remove a waiter that is no longer waiting — granted, expired, or
    /// abandoned because the client hung up.
    pub(super) fn forget(&mut self, ticket: &Ticket) {
        if let Some(queue) = self.waiting.get_mut(&ticket.model) {
            queue.retain(|w| w.seq != ticket.seq);
            if queue.is_empty() {
                self.waiting.remove(&ticket.model);
            }
        }
    }

    /// Decide what `ticket` should do now.
    ///
    /// On [`AdmissionDecision::Serve`] the slot's in-flight count is
    /// incremented and the ticket is dropped from the queue; on
    /// [`AdmissionDecision::Launch`] the slot is marked loading and the ticket
    /// is dropped. Both leave the caller owning exactly one obligation, which
    /// is what keeps the accounting honest.
    pub(super) fn poll(
        &mut self,
        ticket: &Ticket,
        now: Instant,
        secondary: SecondarySlotDecision,
    ) -> AdmissionDecision {
        // The fast path, and the payoff of a second slot: a co-resident model
        // serves without displacing anything, so it answers to the capacity of
        // its own slot and not to the scheduler's fairness rules.
        if let Some(slot) = self.resident_slot(&ticket.model) {
            if !self.may_serve(slot, now) {
                // Held back by the slot's capacity, or standing aside for a
                // rival the turn rules have promised it to. Either way this
                // request is now genuinely waiting, so the deadline has to be
                // tested here — nothing else on this branch would ever surface
                // a 503, and a capped request would wait forever.
                return self.expire_or_wait(ticket, now);
            }
            self.forget(ticket);
            if let SlotState::Resident(r) = &mut self.slots[slot] {
                r.inflight += 1;
            }
            return AdmissionDecision::Serve { slot };
        }

        // Someone else is already launching what this request wants. Wait for
        // it rather than starting a second copy.
        if self.loading_slot(&ticket.model).is_some() {
            return AdmissionDecision::Wait;
        }

        if self.is_expired(ticket, now) {
            self.forget(ticket);
            return AdmissionDecision::Expired;
        }

        // Global FIFO: the oldest waiter across all models decides which model
        // is up next, so a busy model cannot keep jumping the line.
        if self.oldest_waiting_model() != Some(ticket.model.as_str()) {
            return AdmissionDecision::Wait;
        }

        // The co-residence verdict was computed by the caller *before* the
        // queue's lock was taken — no caller code runs inside this critical
        // section (see the locking notes in `lease.rs`; a callback here once
        // re-entered the queue and deadlocked the daemon). Recorded only when
        // a secondary slot could actually have used it, so the dashboard shows
        // the last verdict that mattered rather than one for a moment when no
        // slot was on offer.
        let secondary_available = self.secondary_slots().any(|slot| {
            matches!(self.slots[slot], SlotState::Empty) || self.slots[slot].is_evictable()
        });
        if secondary_available {
            self.secondary_slot = SecondarySlotStatus::from_decision(secondary);
        }
        let may_co_reside = secondary_available && secondary.is_grant();

        let Some(slot) = self.choose_slot(&ticket.model, now, may_co_reside) else {
            return AdmissionDecision::Wait;
        };

        let evict = self.slots[slot].resident().map(|r| r.model_id);
        if evict.is_some() {
            self.stats.total_swaps += 1;
        }

        self.forget(ticket);
        self.slots[slot] = SlotState::Loading {
            model: ticket.model.clone(),
        };
        self.turn = Some(Turn {
            model: ticket.model.clone(),
            started_at: now,
        });
        // A launch starting is progress for everyone behind it.
        self.record_progress();

        AdmissionDecision::Launch { slot, evict }
    }

    /// Whether this request has outlasted [`ADMISSION_DEADLINE`] — measured
    /// against queue *progress*, not against arrival.
    ///
    /// `&mut self` because deciding this is also bookkeeping: a waiter that
    /// observes progress records having seen it, and a waiter behind an
    /// in-flight launch has its stall clock held at `now`.
    fn is_expired(&mut self, ticket: &Ticket, now: Instant) -> bool {
        // A launch in flight is the opposite of a stall: it is bounded by
        // LAUNCH_TIMEOUT, and both of its outcomes change the queue. This is
        // the cold-start case that motivated stall semantics — the first
        // request on a fresh daemon must wait out the model load, not time
        // out in the queue while the load it needs is landing.
        let loading = self.is_loading();
        let epoch = self.progress_epoch;
        let Some(waiter) = self.waiter_mut(ticket) else {
            // Not in the queue — the ticket was already granted or forgotten,
            // so nothing should be asking. Answer with the old absolute rule
            // rather than guessing.
            return now.duration_since(ticket.created_at) >= ADMISSION_DEADLINE;
        };
        if loading {
            waiter.stalled_since = now;
            return false;
        }
        if waiter.seen_epoch != epoch {
            waiter.seen_epoch = epoch;
            waiter.stalled_since = now;
            return false;
        }
        now.duration_since(waiter.stalled_since) >= ADMISSION_DEADLINE
    }

    /// The queue-side record behind `ticket`, if it is still waiting.
    fn waiter_mut(&mut self, ticket: &Ticket) -> Option<&mut Waiter> {
        self.waiting
            .get_mut(&ticket.model)?
            .iter_mut()
            .find(|w| w.seq == ticket.seq)
    }

    /// Record an event that proves the queue is moving. Every waiter's next
    /// poll resets its stall clock against this.
    fn record_progress(&mut self) {
        self.progress_epoch = self.progress_epoch.wrapping_add(1);
    }

    /// The answer for a request that cannot proceed: give up if it has waited
    /// too long, otherwise go round again.
    fn expire_or_wait(&mut self, ticket: &Ticket, now: Instant) -> AdmissionDecision {
        if self.is_expired(ticket, now) {
            self.forget(ticket);
            return AdmissionDecision::Expired;
        }
        AdmissionDecision::Wait
    }

    /// Whether `slot`'s resident may take on one more request right now.
    ///
    /// Two things can stand in the way, and they bound different halves of the
    /// same failure. The slot may already be serving everything its
    /// llama-server was launched to serve at once, in which case forwarding
    /// another would only queue it *inside* llama-server — invisible to the
    /// dashboard, out of the queue's ordering, and holding `inflight` off zero
    /// for as long as it sits there. Or a rival may have become entitled to the
    /// slot, in which case granting now would re-pin a slot that is one release
    /// away from changing hands.
    fn may_serve(&self, slot: usize, now: Instant) -> bool {
        let Some(resident) = self.slots[slot].resident() else {
            return false;
        };
        resident.inflight < SERVER_PARALLEL && !self.owes_slot_to_rival(slot, now)
    }

    /// Whether `slot` is about to be handed to a waiting rival.
    ///
    /// Only an idle slot can be handed over at all, so a busy one has nothing
    /// to stand aside for. Beyond that this is [`choose_slot`](Self::choose_slot)'s
    /// own test asked on the rival's behalf: is a model waiting that needs a
    /// slot, and have the turn rules stopped protecting the incumbent?
    ///
    /// Without this the cap alone would not be enough. It would create only an
    /// *instant* at zero in-flight, and which of the woken requesters claims
    /// that instant is a race — the incumbent's next request re-pins the slot
    /// about as often as the rival takes it, which is a flaky fix rather than a
    /// fix. Standing aside is what turns [`DRAIN_QUANTUM`] from a bound that
    /// expires into one that actually hands the GPU over.
    ///
    /// Conservative in one direction on purpose: it does not work out *which*
    /// slot the rival would be given, so a rival that could have co-loaded into
    /// an empty secondary makes the primary stand aside for one request longer
    /// than it strictly had to. That costs a little latency. Erring the other
    /// way costs the stall this rule exists to prevent.
    fn owes_slot_to_rival(&self, slot: usize, now: Instant) -> bool {
        if !self.slots[slot].is_evictable() {
            return false;
        }
        let Some(rival) = self.oldest_waiting_model() else {
            return false;
        };
        self.swap_is_permitted(rival, now)
    }

    /// Pick the slot `model` should be launched into, if one can be had.
    ///
    /// Preference order:
    ///
    /// 1. An empty slot — nothing is displaced, so nothing has to be justified.
    /// 2. The secondary slot, when the candidate is small enough to co-reside.
    /// 3. An evictable slot, once the fairness rules permit the swap.
    fn choose_slot(&self, model: &str, now: Instant, may_co_reside: bool) -> Option<usize> {
        // 1. A free slot displaces nothing, so it needs no justification.
        if matches!(self.slots[PRIMARY_SLOT], SlotState::Empty) {
            return Some(PRIMARY_SLOT);
        }
        if may_co_reside
            && let Some(slot) = self
                .secondary_slots()
                .find(|&slot| matches!(self.slots[slot], SlotState::Empty))
        {
            return Some(slot);
        }

        // 2. Otherwise something has to go, which the fairness rules govern.
        if !self.swap_is_permitted(model, now) {
            return None;
        }

        // The secondary is sacrificed before the primary: the primary is the
        // model chat traffic follows, and losing it to an auxiliary model's
        // arrival would be the wrong trade. But only a candidate that actually
        // fits there may take it — a model too large for the second slot must
        // displace the primary or wait, never be squeezed in where it does not
        // belong.
        if may_co_reside
            && let Some(slot) = self
                .secondary_slots()
                .find(|&slot| self.slots[slot].is_evictable())
        {
            return Some(slot);
        }
        self.slots[PRIMARY_SLOT]
            .is_evictable()
            .then_some(PRIMARY_SLOT)
    }

    /// Every slot that is not the primary.
    fn secondary_slots(&self) -> impl Iterator<Item = usize> + use<> {
        PRIMARY_SLOT + 1..SLOT_COUNT
    }

    /// Whether the current turn holder may be displaced.
    ///
    /// With no turn recorded, or with the turn belonging to a model that is no
    /// longer resident, there is nothing to protect.
    fn swap_is_permitted(&self, challenger: &str, now: Instant) -> bool {
        let Some(turn) = &self.turn else {
            return true;
        };
        if turn.model == challenger {
            return true;
        }
        // A turn holder with nothing left queued has finished its batch.
        if !self.waiting.contains_key(&turn.model) {
            return true;
        }
        now.duration_since(turn.started_at) >= DRAIN_QUANTUM
    }

    /// Which slot holds `model`, if any.
    fn resident_slot(&self, model: &str) -> Option<usize> {
        (0..SLOT_COUNT).find(|&slot| {
            self.slots[slot]
                .resident()
                .is_some_and(|r| r.model_name == model)
        })
    }

    /// Which slot is mid-launch for `model`, if any.
    fn loading_slot(&self, model: &str) -> Option<usize> {
        (0..SLOT_COUNT).find(
            |&slot| matches!(&self.slots[slot], SlotState::Loading { model: m } if m == model),
        )
    }

    /// The model whose oldest waiter arrived first, among those that actually
    /// need a slot.
    ///
    /// Models that are already resident are skipped: their waiters are served
    /// on their very next poll without the scheduler being consulted at all, so
    /// letting one hold the front of the line would block a model that does
    /// need a decision behind a request that never wanted one.
    ///
    /// Ordered by arrival sequence rather than elapsed time: the two agree, and
    /// a monotonic counter cannot be perturbed by a clock the tests pause.
    fn oldest_waiting_model(&self) -> Option<&str> {
        self.waiting
            .iter()
            .filter(|(model, _)| self.resident_slot(model).is_none())
            .filter_map(|(model, queue)| queue.front().map(|w| (w.seq, model.as_str())))
            .min_by_key(|(seq, _)| *seq)
            .map(|(_, model)| model)
    }

    /// Record that a launch completed and `resident` now occupies `slot`.
    pub(super) fn install(&mut self, slot: usize, resident: Resident) {
        self.slots[slot] = SlotState::Resident(Box::new(resident));
        self.record_progress();
    }

    /// Record that a launch into `slot` failed, freeing it for another attempt.
    pub(super) fn launch_failed(&mut self, slot: usize) {
        if matches!(self.slots[slot], SlotState::Loading { .. }) {
            self.slots[slot] = SlotState::Empty;
        }
        self.record_progress();
    }

    /// Release one in-flight reference to `slot`.
    pub(super) fn release(&mut self, slot: usize) {
        if let Some(SlotState::Resident(r)) = self.slots.get_mut(slot) {
            r.inflight = r.inflight.saturating_sub(1);
        }
        // A generation finished. The waiter this matters to may lose the
        // freed capacity to a racing re-acquire before its next poll, so the
        // epoch — not the resulting state — is what tells it the queue moved.
        self.record_progress();
    }

    /// Increment the in-flight count for `slot`, when a caller has re-acquired
    /// a lease on a model it already knows is resident.
    pub(super) fn retain(&mut self, slot: usize) -> bool {
        match self.slots.get_mut(slot) {
            Some(SlotState::Resident(r)) => {
                r.inflight += 1;
                true
            }
            _ => false,
        }
    }

    /// Empty a slot, unconditionally. Used by explicit stop requests, which are
    /// the user's decision rather than the scheduler's.
    pub(super) fn evict(&mut self, slot: usize) -> Option<Resident> {
        let previous = std::mem::replace(&mut self.slots[slot], SlotState::Empty);
        self.record_progress();
        match previous {
            SlotState::Resident(r) => Some(*r),
            SlotState::Empty | SlotState::Loading { .. } => None,
        }
    }

    /// The resident in `slot`, if any.
    pub(super) fn slot(&self, slot: usize) -> Option<&Resident> {
        self.slots.get(slot).and_then(SlotState::resident)
    }

    /// The primary slot's resident, which is what `current_model` reports.
    pub(super) fn primary(&self) -> Option<&Resident> {
        self.slot(PRIMARY_SLOT)
    }

    /// Every resident, primary first.
    pub(super) fn residents(&self) -> impl Iterator<Item = (usize, &Resident)> {
        (0..SLOT_COUNT).filter_map(|slot| self.slot(slot).map(|r| (slot, r)))
    }

    /// Whether any slot is mid-launch.
    pub(super) fn is_loading(&self) -> bool {
        self.slots
            .iter()
            .any(|s| matches!(s, SlotState::Loading { .. }))
    }

    /// Which slot holds or is loading `model`.
    pub(super) fn slot_of(&self, model: &str) -> Option<usize> {
        (0..SLOT_COUNT).find(|&slot| self.slots[slot].model() == Some(model))
    }

    /// Project the queue for the dashboard.
    pub(super) fn snapshot(&self, now: Instant) -> AdmissionSnapshot {
        let mut queued: Vec<QueuedModelSnapshot> = self
            .waiting
            .iter()
            .filter(|(_, q)| !q.is_empty())
            .map(|(model, q)| QueuedModelSnapshot {
                model_name: model.clone(),
                waiting: q.len(),
                oldest_wait_ms: q
                    .front()
                    .map_or(0, |w| now.duration_since(w.enqueued_at).as_millis() as u64),
            })
            .collect();
        // Longest-waiting first: the entry a user needs to see is the one that
        // is about to force a swap.
        queued.sort_by_key(|q| std::cmp::Reverse(q.oldest_wait_ms));

        AdmissionSnapshot {
            slots: self
                .residents()
                .map(|(slot, r)| ResidentSlotSnapshot {
                    slot,
                    model_name: r.model_name.clone(),
                    model_id: r.model_id,
                    port: r.port,
                    inflight: r.inflight,
                    is_primary: slot == PRIMARY_SLOT,
                    resident_for_secs: now.duration_since(r.resident_since).as_secs(),
                })
                .collect(),
            queued,
            total_queued: self.stats.total_queued,
            total_swaps: self.stats.total_swaps,
            secondary_slot: self.slot(PRIMARY_SLOT + 1).map_or_else(
                || self.secondary_slot.clone(),
                |r| SecondarySlotStatus::resident(&r.model_name),
            ),
        }
    }
}
