//! How long a turn, a launch and a wait may last.
//!
//! Split from `state.rs`, whose rules read them, so the budgets and the
//! reasoning behind each sit together.

use std::time::Duration;

/// How long a model keeps the GPU once it has won a turn, while another model
/// is waiting.
///
/// The value trades swap amortisation against rival latency. A swap costs
/// roughly 5–15 s (process teardown, weight load, health check), so a quantum
/// materially shorter than that would spend more time swapping than serving.
/// Twenty seconds drains a healthy burst while keeping the rival's wait inside
/// what an agentic client will sit through.
pub const DRAIN_QUANTUM: Duration = Duration::from_secs(20);

/// Everything in a launch that is not waiting for the server to answer.
///
/// Model resolution, the *displaced* process's shutdown — which is inline and
/// awaited, and can itself take SIGTERM plus a five-second grace — the purge of
/// stale slot files, and the spawn.
///
/// Deliberately not headroom for the failed-launch cleanup: that runs from a
/// `Drop` on a detached task, outside this budget by construction, because the
/// case it exists for is the one where this budget has already expired.
pub(crate) const LAUNCH_OVERHEAD: Duration = Duration::from_secs(40);

/// How long one model launch may take before it is abandoned.
///
/// Derived per launch rather than fixed, because the health wait it contains
/// is itself sized to the model — see `process::health::launch_deadline_secs`.
/// A flat budget would silently cap that wait: a model whose deadline exceeded
/// it would have its future dropped by this timeout first, skipping the
/// cleanup inside it and leaking the very process the deadline was extended
/// for.
#[must_use]
pub(crate) const fn launch_timeout(health_deadline: Duration) -> Duration {
    health_deadline.saturating_add(LAUNCH_OVERHEAD)
}

/// How long a request may wait **with no queue progress** before giving up
/// with a 503.
///
/// A *stall* deadline, not a wall clock: the clock measures time since the
/// queue last did anything on anyone's behalf, so a first request on a cold
/// daemon waiting out a model load that is proceeding normally does not expire
/// the way a wedged queue does. Two things hold it back:
///
/// - **A launch in flight pauses every waiter's clock.** A loading slot is
///   the opposite of a stall, and it is bounded on its own: [`launch_timeout`]
///   abandons an overrunning launch and frees the slot, at which point the
///   clock runs again. Waiting out a load can therefore extend a wait by at
///   most one launch budget at a time, never indefinitely.
///
///   That budget scales with the model: [`launch_timeout`] runs from 160 s to
///   640 s. So a large model loading can pause every other waiter's clock for
///   well past this deadline. That is consistent with what the clock measures
///   — a load in flight is progress, not a stall — but whether a waiter behind
///   a ten-minute load should have its clock paused outright rather than
///   merely extended is a question this constant does not answer.
/// - **Progress resets the clock.** A lease released (a generation finished),
///   a launch landing or failing, a slot evicted — each proves the queue is
///   moving, so a waiter behind it is queued, not stuck.
///
/// [`DRAIN_QUANTUM`] bounds how long a *turn* lasts, but a turn cannot end
/// while the outgoing model still has requests in flight — no swap may preempt
/// a live generation. A single generation is indivisible, so the rival behind
/// it has no bound but this one: an in-flight count that stays pinned with no
/// release for this long is a hog or a wedge, and the waiter behind it gets
/// its 503.
///
/// A model under *overlapping* load cannot hold its slot indefinitely:
/// `SERVER_PARALLEL` caps how many requests are in flight at once, and
/// `owes_slot_to_rival` makes an idle slot stand aside for a waiting rival. So
/// reaching this deadline is the exception, not the ordinary outcome of two
/// clients sharing an endpoint.
pub const ADMISSION_DEADLINE: Duration = Duration::from_mins(3);
