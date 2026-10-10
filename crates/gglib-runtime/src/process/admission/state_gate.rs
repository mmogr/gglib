//! The generation gate's rules: whose turn it is to generate.
//!
//! A `#[path]` child of `state.rs`, so it reads the queue's private fields.
//! The gate lives inside [`QueueState`] on purpose: one lock, one `Notify`,
//! and arrival numbers from the same counter as admission tickets, so a gate
//! waiter and a ticket can be put in one order.
//!
//! The rules (the plumbing that waits on them is `gate.rs`):
//!
//! - **What counts.** The LLM turns in flight are the explicit LLM turns plus
//!   every in-flight request on a llama-server resident. A lease on an
//!   `sd-server` resident never counts, and the fast path ignores the gate
//!   for one, so a render cannot wait on its own lease.
//! - **Order.** A render starts when no render is held, no LLM turn is in
//!   flight, and no gate waiter arrived before it. An LLM turn, and a
//!   llama-server serve or launch, may go when no render is held and no
//!   render waiter arrived before it, so new chats queue behind a waiting
//!   render and cannot starve it.
//! - **Tickets are not turns.** A request waiting for a slot does not block a
//!   render: an older chat waiting to evict the very slot a waiting render's
//!   lease pins would otherwise wait on the render while the render waited
//!   on it.
//! - **Progress.** A render step counts as queue progress (through its
//!   lease), and a gate waiter expires under the same stall rule as a ticket,
//!   [`ADMISSION_DEADLINE`] with nothing moving.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use gglib_core::domain::{GenerationSnapshot, RenderSnapshot, RuntimeKind};
use gglib_core::ports::{GateWait, TurnKind, WaitReason};
use tokio::time::Instant;

use super::{ADMISSION_DEADLINE, QueueState, Resident, SlotState};

/// The gate's part of the queue's state.
#[derive(Debug, Default)]
pub(super) struct GateState {
    /// The render that holds the GPU, if one does.
    render: Option<HeldRender>,
    /// The explicit LLM turns in flight, by id.
    llm: HashSet<u64>,
    /// Who is waiting for a turn, oldest first.
    waiters: VecDeque<GateWaiter>,
    /// The id the next granted turn gets.
    next_id: u64,
}

/// The render holding the GPU, and how far it has got.
#[derive(Debug)]
struct HeldRender {
    id: u64,
    /// The slot its lease pins, which holds the image model drawing it.
    slot: Option<usize>,
    step: u32,
    total: u32,
}

/// One caller waiting for a turn.
#[derive(Debug)]
struct GateWaiter {
    seq: u64,
    kind: TurnKind,
    /// When this waiter last saw the queue move; see [`ADMISSION_DEADLINE`].
    stalled_since: Instant,
    /// The progress epoch this waiter has accounted for.
    seen_epoch: u64,
}

/// A place in the gate's line, held by the caller for the whole wait.
#[derive(Debug)]
pub(in crate::process::admission) struct GateTicket {
    seq: u64,
    kind: TurnKind,
    /// A render's lease's slot; `None` for an LLM turn.
    slot: Option<usize>,
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
    /// when a render is in the way, and nothing when only LLM turns are.
    Wait(Option<GateWait>),
    /// Nothing moved for the whole deadline; the waiter is gone from the
    /// line.
    Stalled(Duration),
}

impl QueueState {
    /// Take a place in the gate's line for a turn of `kind`; a render names
    /// the slot its lease pins.
    pub(in crate::process::admission) fn gate_enqueue(
        &mut self,
        kind: TurnKind,
        slot: Option<usize>,
        now: Instant,
    ) -> GateTicket {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.gate.waiters.push_back(GateWaiter {
            seq,
            kind,
            stalled_since: now,
            seen_epoch: self.progress_epoch,
        });
        GateTicket {
            seq,
            kind,
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
        let may_go = match ticket.kind {
            TurnKind::Llm => self.gate_admits_llm(ticket.seq),
            TurnKind::Render => self.render_may_start(ticket.seq),
        };
        if may_go {
            self.gate_forget(ticket);
            let id = self.gate.next_id;
            self.gate.next_id += 1;
            match ticket.kind {
                TurnKind::Llm => {
                    self.gate.llm.insert(id);
                }
                TurnKind::Render => {
                    self.gate.render = Some(HeldRender {
                        id,
                        slot: ticket.slot,
                        step: 0,
                        total: 0,
                    });
                }
            }
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
        let render_ahead = self
            .gate
            .waiters
            .iter()
            .any(|w| w.kind == TurnKind::Render && w.seq < seq);
        if self.gate.render.is_none() && !render_ahead {
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

    /// Whether an LLM turn, or a llama-server serve or launch, arriving as
    /// `seq` may go: no render held, and none waiting that arrived first.
    pub(super) fn gate_admits_llm(&self, seq: u64) -> bool {
        self.gate.render.is_none()
            && !self
                .gate
                .waiters
                .iter()
                .any(|w| w.kind == TurnKind::Render && w.seq < seq)
    }

    /// Whether the render waiter `seq` may start: no render held, no LLM
    /// turn in flight, and no gate waiter ahead of it. Tickets waiting for a
    /// slot are not consulted.
    fn render_may_start(&self, seq: u64) -> bool {
        self.gate.render.is_none()
            && self.llm_inflight() == 0
            && !self.gate.waiters.iter().any(|w| w.seq < seq)
    }

    /// The LLM turns in flight: explicit turns plus every in-flight request on
    /// a llama-server resident. A lease on an `sd-server` resident is not one.
    fn llm_inflight(&self) -> u32 {
        let leases: u32 = self
            .residents()
            .filter(|(_, r)| r.runtime == RuntimeKind::Llama)
            .map(|(_, r)| r.inflight)
            .sum();
        leases.saturating_add(u32::try_from(self.gate.llm.len()).unwrap_or(u32::MAX))
    }

    /// The gate projected for the dashboard.
    pub(super) fn generation_snapshot(&self) -> GenerationSnapshot {
        GenerationSnapshot {
            render: self.gate.render.as_ref().map(|r| RenderSnapshot {
                model_name: r
                    .slot
                    .and_then(|slot| self.slot(slot))
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

    /// The turn `id` has ended.
    pub(in crate::process::admission) fn gate_end(&mut self, id: u64) {
        if self.gate.render.as_ref().is_some_and(|r| r.id == id) {
            self.gate.render = None;
        } else {
            self.gate.llm.remove(&id);
        }
        // A turn ending is what everyone behind it was waiting for.
        self.record_progress();
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
