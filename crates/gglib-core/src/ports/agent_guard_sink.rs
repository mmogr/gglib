//! Outbound port for the agent loop's guard decisions.
//!
//! The agent loop runs the same two detectors the proxy does — `LoopDetector`
//! and `StagnationDetector`, both from [`crate::domain::agent`] — and until
//! #1091 its decisions reached neither the per-model ledger nor the guard's
//! persisted log, so every number either of those gave about the loop guard
//! described one of its two callers. A trip was not invisible — it ends the
//! run, and the error event says so to whoever is watching. Two benchmark
//! harnesses *record* it: the tuning and agentic evals share
//! `run_task_with_llm`, which turns that error into a per-task
//! `loop_detected` flag, and each rolls those up into a `loop_avoidance`
//! axis above it. Both score a run under test. What nothing had was a count
//! of what the guard does in service.
//!
//! This is the seam that fixes that. `gglib-agent` must not depend on the
//! proxy, so the hand-over is a port here, the shape
//! [`UsageSink`](super::UsageSink) already established: synchronous, holding
//! nothing the caller waits on, and held as an `Option` by the loop so that a
//! process with nothing to report to makes recording a no-op.
//!
//! # Why not [`LoopGuardTripSink`](super::LoopGuardTripSink)
//!
//! That port is the proxy's persisted log, and it takes a
//! [`LoopGuardTripEvent`](crate::domain::loop_guard_log::LoopGuardTripEvent)
//! keyed by the guard's [`LoopGuardMode`](crate::settings::LoopGuardMode) —
//! `note` or `refuse`, the proxy setting that decides what happens to a
//! tripped request. The agent path has no such mode: its guard comes from
//! `AgentConfig`, and a trip there always ends the run. Writing agent
//! decisions through that port would file them under a mode they were never
//! taken under.
//!
//! The two ports may meet later — the log is where ADR 0011's kill criterion
//! reads, and giving it a path column is the sequel to this work — but that
//! is a decision about the log's schema, not about this seam.
//!
//! # Why every decision, and not only the trips
//!
//! A trip count with no denominator is the unreadable instrument this port
//! exists to replace. The proxy's trips sit over `requests`, which it records
//! for every request it forwarded or would have but for a guard; the agent
//! loop had no such count of its own, so agent trips alone would be a
//! numerator over nothing. So the sink is told about
//! **every** turn the guard ran on, and `None` — both detectors quiet — is
//! the ordinary case.

use std::sync::Arc;

use crate::domain::defects::LoopGuardTrip;

/// Where the agent loop reports what its guard decided.
///
/// Called on the loop's own path, once per turn the guard ran on, so an
/// implementation must return at once: bump a counter and move on. It must
/// not block, must not fail, and must never turn a recording problem into a
/// problem for the run being guarded — there is no error to return, by
/// design.
pub trait AgentGuardSink: Send + Sync {
    /// Record one guard decision for `model`.
    ///
    /// `trip` is `None` when the guard ran and neither detector fired, which
    /// is what makes the count a denominator rather than a tally of failures.
    /// `Some(which)` says which detector ended the run.
    fn record_decision(&self, model: &str, trip: Option<LoopGuardTrip>);
}

/// A sink and the model name to record under, travelling together.
///
/// The two are one value because neither is any use alone: a sink with no
/// model name has nothing to key on, and a model name with no sink has
/// nowhere to go. Passing them separately would let a caller supply one and
/// not the other, and the compiler would not mind.
///
/// The name is resolved by whoever composes the loop, because only they know
/// it. A request that named a model is counted under that name; one that
/// named none — the ordinary local case, meaning "whatever the server has
/// loaded" — is counted under the name of the model actually running on the
/// port it was sent to.
pub struct AgentGuardReporter {
    /// Where the decisions go.
    pub sink: Arc<dyn AgentGuardSink>,
    /// The model name every decision from this run is recorded under.
    pub model: String,
}
