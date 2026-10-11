//! The generation gate: whose turn it is to generate on the GPU.
//!
//! An image render on `sd-server` and an LLM generation on `llama-server`
//! share one GPU, and the admission queue alone cannot order them: it decides
//! which models are resident, not which resident is generating. A render
//! needs the GPU to itself for minutes; LLM generations may run together. The
//! gate orders them as turns, first come first served. A render waits for the
//! LLM turns in flight to drain, and new LLM turns queue behind a waiting
//! render, so a stream of short chats cannot starve it.
//!
//! Only leases on llama-server residents count as LLM turns. A render takes
//! its `sd-server` lease first and hands it to [`GenerationGate::render_turn`],
//! so the turn owns the lease and the two end together; the lease never
//! counts against the render it belongs to.
//!
//! `gglib-runtime`'s admission queue implements it, under the queue's own
//! lock, with no caller code run under that lock.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::ports::AdmissionLease;

/// What a turn is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnKind {
    /// An LLM generation. Any number may run together.
    Llm,
    /// An image render, which runs alone.
    Render,
}

/// Why a turn is being waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    /// An image render holds the GPU, or is queued ahead.
    ImageRender,
}

/// One report to a [`GateWaitObserver`] while a turn is waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateWait {
    /// What is being waited on.
    pub reason: WaitReason,
    /// The step the render in the way last reported; 0 before its first.
    pub step: u32,
    /// How many steps that render takes; 0 before its first report.
    pub total: u32,
    /// This waiter's place in the gate's line, 1 being next.
    pub position: usize,
}

/// Told how a wait for a turn is going, so a person can see why nothing is
/// happening yet.
///
/// Called with no lock held, after the gate has answered "wait", and again
/// whenever what it reports changes. Must not block: a slow observer delays
/// the wait it is describing.
pub trait GateWaitObserver: Send + Sync + fmt::Debug {
    /// The turn is still being waited for.
    fn waiting(&self, wait: GateWait);
}

/// Why no turn was granted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GateError {
    /// Nothing moved for the whole admission deadline: no generation
    /// finished, no render stepped, no model loaded. A render step resets
    /// the clock, so a long render that keeps stepping never causes this.
    #[error(
        "waited {}s for a generation turn while nothing finished and no render stepped",
        .0.as_secs()
    )]
    Stalled(Duration),
}

/// The gate side of a [`GenerationTurn`]: what to call as the turn makes
/// progress and when it ends.
///
/// Called from the turn's `Drop` and from inside a render's progress loop, so
/// neither method may block, panic or await.
pub trait GateRelease: Send + Sync + fmt::Debug {
    /// Turn `id`, a render, has reached `step` of `total`.
    fn progress(&self, id: u64, step: u32, total: u32);

    /// Turn `id` has ended.
    fn end(&self, id: u64);
}

/// A granted turn on the gate. Dropping it ends the turn.
///
/// A render turn owns the `sd-server` lease it was granted on, so a task that
/// keeps the turn (a render the user cancelled but `sd-server` cannot stop)
/// keeps the model resident by keeping one value. On drop the turn ends
/// first, then the lease is released.
///
/// Not `Clone`: two owners would end one turn twice.
#[derive(Debug)]
pub struct GenerationTurn {
    owner: Option<Arc<dyn GateRelease>>,
    id: u64,
    kind: TurnKind,
    // Declared last so it drops after `Drop::drop` has ended the turn.
    lease: Option<AdmissionLease>,
}

impl GenerationTurn {
    /// A turn that ends on `owner` when dropped, holding `lease` until then.
    #[must_use]
    pub fn new(
        owner: Arc<dyn GateRelease>,
        id: u64,
        kind: TurnKind,
        lease: Option<AdmissionLease>,
    ) -> Self {
        Self {
            owner: Some(owner),
            id,
            kind,
            lease,
        }
    }

    /// The gate's id for this turn.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// What this turn is for.
    #[must_use]
    pub const fn kind(&self) -> TurnKind {
        self.kind
    }

    /// The lease this turn holds, if it still holds one.
    #[must_use]
    pub const fn lease(&self) -> Option<&AdmissionLease> {
        self.lease.as_ref()
    }

    /// The render has reached `step` of `total`: kept for anyone waiting
    /// behind it, and counted as queue progress through the lease, so their
    /// stall clocks start again.
    pub fn progress(&self, step: u32, total: u32) {
        if let Some(owner) = &self.owner {
            owner.progress(self.id, step, total);
        }
        if let Some(lease) = &self.lease {
            lease.progress();
        }
    }

    /// Take the lease out of the turn, for a teardown that settles the
    /// lease's count itself and must still end the turn after it.
    pub fn take_lease(&mut self) -> Option<AdmissionLease> {
        self.lease.take()
    }
}

impl Drop for GenerationTurn {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            owner.end(self.id);
        }
    }
}

/// The daemon-wide generation gate (see the [module docs](self)).
#[async_trait]
pub trait GenerationGate: Send + Sync + fmt::Debug {
    /// Wait for an LLM turn.
    ///
    /// # Errors
    ///
    /// [`GateError::Stalled`] when nothing moves for the admission deadline.
    async fn llm_turn(
        &self,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError>;

    /// Wait for a render turn on the `sd-server` resident `lease` holds. The
    /// lease is taken first, so the model is resident and cannot be swapped
    /// out while the render waits; it never counts as an LLM turn.
    ///
    /// # Errors
    ///
    /// [`GateError::Stalled`] when nothing moves for the admission deadline;
    /// the lease is released.
    async fn render_turn(
        &self,
        lease: AdmissionLease,
        observer: Option<Arc<dyn GateWaitObserver>>,
    ) -> Result<GenerationTurn, GateError>;
}

#[cfg(test)]
#[path = "generation_gate_tests.rs"]
mod tests;
