//! The generation gate's rules: whose turn it is to generate.
//!
//! A `#[path]` child of `state.rs`, so it reads the queue's private fields.
//! The gate lives inside [`QueueState`] on purpose: one lock, one `Notify`,
//! and arrival numbers from the same counter as admission tickets, so a gate
//! waiter and a ticket can be put in one order.
//!
//! The rules (the plumbing that waits on them is `gate.rs`):
//!
//! - **What counts.** The LLM requests in flight are the in-flight requests
//!   on llama-server residents. A lease on an `sd-server` resident never
//!   counts, and the fast path ignores the gate for one, so a render cannot
//!   wait on its own lease.
//! - **Order.** Only renders wait in the gate's line. A render starts when no
//!   render is held, no LLM request is in flight, and no render arrived
//!   before it. A llama-server serve or launch may go when no render is held
//!   and none arrived before it, so new chats queue behind a waiting render
//!   and cannot starve it.
//! - **Tickets are not turns.** A request waiting for a slot does not block a
//!   render: an older chat waiting to evict the very slot a waiting render's
//!   lease pins would otherwise wait on the render while the render waited
//!   on it.
//! - **A Stop asks.** A person's Stop on the image model a render holds, or
//!   waits in line with, never empties the slot under the render's lease: it
//!   is noted here ([`QueueState::gate_ask_render_stop`]). A waiting render
//!   leaves the line with its lease; one that is drawing is retired by its
//!   driver, which reads the note at its next look at the job.
//! - **Progress.** A render step counts as queue progress (through its
//!   lease), and a gate waiter expires under the same stall rule as a ticket,
//!   [`ADMISSION_DEADLINE`] with nothing moving.

use std::collections::VecDeque;
use std::time::Duration;

use gglib_core::domain::{GenerationSnapshot, RenderSnapshot, RuntimeKind};
use gglib_core::ports::{GateWait, WaitReason};
use tokio::time::Instant;

use super::{ADMISSION_DEADLINE, QueueState, Resident, SlotState};

/// The gate's part of the queue's state.
#[derive(Debug, Default)]
pub(super) struct GateState {
    /// The render that holds the GPU, if one does.
    render: Option<HeldRender>,
    /// The renders waiting for a turn, oldest first.
    waiters: VecDeque<GateWaiter>,
    /// The id the next granted turn gets.
    next_id: u64,
}

/// The render holding the GPU, and how far it has got.
#[derive(Debug)]
struct HeldRender {
    id: u64,
    /// The slot its lease pins, which holds the image model drawing it.
    slot: usize,
    step: u32,
    total: u32,
    /// A Stop was asked of the model in `slot` while this render held it.
    stop_asked: bool,
}

/// One render waiting for its turn.
#[derive(Debug)]
struct GateWaiter {
    seq: u64,
    /// When this waiter last saw the queue move; see [`ADMISSION_DEADLINE`].
    stalled_since: Instant,
    /// The progress epoch this waiter has accounted for.
    seen_epoch: u64,
    /// The slot its lease pins.
    slot: usize,
    /// A Stop was asked of the model in `slot` while this render waited.
    stop_asked: bool,
}

/// A place in the gate's line, held by the caller for the whole wait.
#[derive(Debug)]
pub(in crate::process::admission) struct GateTicket {
    seq: u64,
    /// The slot the render's lease pins.
    slot: usize,
    created_at: Instant,
}

/// What a caller waiting for a turn should do next.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::process::admission) enum GateVerdict {
    /// The turn is granted, with this id; the waiter is gone from the line.
    Granted {
        /// The turn's id, which ends it.
        id: u64,
    },
    /// Wait for a wakeup and ask again. Carries what to tell an observer
    /// when a render is in the way, and nothing when only LLM requests are.
    Wait(Option<GateWait>),
    /// Nothing moved for the whole deadline; the waiter is gone from the
    /// line.
    Stalled(Duration),
    /// The model this render's lease pins was asked to stop; the waiter is
    /// gone from the line.
    Stopped,
}

impl QueueState {
    /// Take a place in the gate's line for a render whose lease pins `slot`.
    pub(in crate::process::admission) fn gate_enqueue(
        &mut self,
        slot: usize,
        now: Instant,
    ) -> GateTicket {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.gate.waiters.push_back(GateWaiter {
            seq,
            stalled_since: now,
            seen_epoch: self.progress_epoch,
            slot,
            stop_asked: false,
        });
        GateTicket {
            seq,
            slot,
            created_at: now,
        }
    }

    /// Leave the gate's line; a no-op once the turn was granted.
    pub(in crate::process::admission) fn gate_forget(&mut self, ticket: &GateTicket) {
        self.gate.waiters.retain(|w| w.seq != ticket.seq);
    }

    /// Decide what the caller behind `ticket` should do now.
    pub(in crate::process::admission) fn gate_poll(
        &mut self,
        ticket: &GateTicket,
        now: Instant,
    ) -> GateVerdict {
        let stopped = |w: &GateWaiter| w.seq == ticket.seq && w.stop_asked;
        if self.gate.waiters.iter().any(stopped) {
            self.gate_forget(ticket);
            return GateVerdict::Stopped;
        }
        if self.render_may_start(ticket.seq) {
            self.gate_forget(ticket);
            let id = self.gate.next_id;
            self.gate.next_id += 1;
            self.gate.render = Some(HeldRender {
                id,
                slot: ticket.slot,
                step: 0,
                total: 0,
                stop_asked: false,
            });
            return GateVerdict::Granted { id };
        }
        if let Some(waited) = self.gate_stalled(ticket, now) {
            self.gate_forget(ticket);
            return GateVerdict::Stalled(waited);
        }
        GateVerdict::Wait(self.behind_a_render(ticket.seq))
    }

    /// How long `ticket` has waited with nothing moving, once that reaches
    /// [`ADMISSION_DEADLINE`]: the same stall rule a ticket answers to.
    fn gate_stalled(&mut self, ticket: &GateTicket, now: Instant) -> Option<Duration> {
        let loading = self.is_loading();
        let epoch = self.progress_epoch;
        let Some(waiter) = self.gate.waiters.iter_mut().find(|w| w.seq == ticket.seq) else {
            let stalled_for = now.duration_since(ticket.created_at);
            return (stalled_for >= ADMISSION_DEADLINE).then_some(stalled_for);
        };
        if loading || waiter.seen_epoch != epoch {
            waiter.seen_epoch = epoch;
            waiter.stalled_since = now;
            return None;
        }
        let stalled_for = now.duration_since(waiter.stalled_since);
        (stalled_for >= ADMISSION_DEADLINE).then_some(stalled_for)
    }

    /// What an observer of the waiter `seq` is told, when a render is in its
    /// way: one held, or one queued ahead of it.
    fn behind_a_render(&self, seq: u64) -> Option<GateWait> {
        if self.gate.render.is_none() && !self.render_waits_before(seq) {
            return None;
        }
        let (step, total) = self
            .gate
            .render
            .as_ref()
            .map_or((0, 0), |r| (r.step, r.total));
        Some(GateWait {
            reason: WaitReason::ImageRender,
            step,
            total,
            position: 1 + self.gate.waiters.iter().filter(|w| w.seq < seq).count(),
        })
    }

    /// Whether a render that arrived before `seq` waits in the gate's line.
    fn render_waits_before(&self, seq: u64) -> bool {
        self.gate.waiters.iter().any(|w| w.seq < seq)
    }

    /// Whether a llama-server serve or launch arriving as `seq` may go: no
    /// render held, and none waiting that arrived first.
    pub(super) fn gate_admits_llm(&self, seq: u64) -> bool {
        self.gate.render.is_none() && !self.render_waits_before(seq)
    }

    /// Whether the render waiter `seq` may start: no render held, no LLM
    /// request in flight, and no render ahead of it. Tickets waiting for a
    /// slot are not consulted.
    fn render_may_start(&self, seq: u64) -> bool {
        self.gate.render.is_none() && self.llm_inflight() == 0 && !self.render_waits_before(seq)
    }

    /// The LLM requests in flight: every in-flight request on a llama-server
    /// resident. A lease on an `sd-server` resident is not one.
    fn llm_inflight(&self) -> u32 {
        self.residents()
            .filter(|(_, r)| r.runtime == RuntimeKind::Llama)
            .map(|(_, r)| r.inflight)
            .sum()
    }

    /// The gate projected for the dashboard.
    pub(super) fn generation_snapshot(&self) -> GenerationSnapshot {
        GenerationSnapshot {
            render: self.gate.render.as_ref().map(|r| RenderSnapshot {
                model_name: self
                    .slot(r.slot)
                    .map(|resident| resident.model_name.clone()),
                step: r.step,
                total: r.total,
            }),
            llm_inflight: self.llm_inflight(),
            waiting: self.gate.waiters.len(),
        }
    }

    /// The render turn `id` has reached `step` of `total`.
    pub(in crate::process::admission) fn gate_progress(&mut self, id: u64, step: u32, total: u32) {
        if let Some(render) = self.gate.render.as_mut().filter(|r| r.id == id) {
            render.step = step;
            render.total = total;
        }
    }

    /// The render turn `id` has ended.
    pub(in crate::process::admission) fn gate_end(&mut self, id: u64) {
        if self.gate.render.as_ref().is_some_and(|r| r.id == id) {
            self.gate.render = None;
        }
        // A turn ending is what everyone behind it was waiting for.
        self.record_progress();
    }

    /// Note a Stop of model `model_id` on every render whose lease pins the
    /// slot holding it, the one drawing and any waiting in line. Answers
    /// whether there was one, and whether any of them had not been asked
    /// before. The slot is left as it is: emptying it is the render's to do
    /// ([`Self::retire`]), or the Stop's once no render holds the model.
    pub(in crate::process::admission) fn gate_ask_render_stop(
        &mut self,
        model_id: u32,
    ) -> (bool, bool) {
        let Some(slot) = self.slot_holding(model_id) else {
            return (false, false);
        };
        let held = self.gate.render.as_mut().filter(|r| r.slot == slot);
        let waiting = self.gate.waiters.iter_mut().filter(|w| w.slot == slot);
        let (mut asked, mut news) = (false, false);
        for stop_asked in held
            .map(|r| &mut r.stop_asked)
            .into_iter()
            .chain(waiting.map(|w| &mut w.stop_asked))
        {
            asked = true;
            news |= !*stop_asked;
            *stop_asked = true;
        }
        (asked, news)
    }

    /// Whether a Stop was asked of model `model_id` while the render that
    /// holds the GPU draws with it.
    pub(in crate::process::admission) fn gate_render_stop_asked(&self, model_id: u32) -> bool {
        let Some(slot) = self.slot_holding(model_id) else {
            return false;
        };
        let held = |r: &HeldRender| r.slot == slot && r.stop_asked;
        self.gate.render.as_ref().is_some_and(held)
    }

    /// The slot whose resident is model `model_id`.
    fn slot_holding(&self, model_id: u32) -> Option<usize> {
        self.residents()
            .find(|(_, r)| r.model_id == model_id)
            .map(|(slot, _)| slot)
    }

    /// Retire a render's resident after its process was killed: empty
    /// `slot`, which settles the render's lease count with the resident it
    /// was counted against, but only while `slot` still holds `model_id`.
    /// Once anything else has taken the slot, a release there would take a
    /// request away from that newcomer, so this does nothing; the caller
    /// disarms the lease either way.
    pub(in crate::process::admission) fn retire(
        &mut self,
        slot: usize,
        model_id: u32,
    ) -> Option<Resident> {
        let SlotState::Resident(resident) = self.slots.get(slot)? else {
            return None;
        };
        if resident.model_id != model_id {
            return None;
        }
        self.evict(slot)
    }
}
