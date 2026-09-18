//! Per-model defect counters — the Tier C signals the closed loop steers by.
//!
//! The proxy records defect *events* (a loop-guard trip, a tool-call repair,
//! a turn that died mid-stream) as they happen; a reader turns those into
//! *rates* over whatever window it cares about. Writers never interpret and
//! readers never guess: a trip is a fact about one request, a rate is a
//! claim about a model, and the split keeps both honest.
//!
//! Counters are cumulative and process-lifetime (they live on the proxy
//! supervisor, like the agent cache metrics, so a proxy restart does not
//! zero them). There is no windowing here, and no `delta` helper: that pair
//! existed for the tune scheduler, which kept per-model baselines and rated
//! the difference. Since ADR 0006 nothing acts on these automatically, and the
//! one reader left — `gglib proxy dashboard` — shows the run's totals, which
//! is the honest shape for a counter that resets with the process.
//!
//! They are diagnosis: what actually fails, per model, for a person to read
//! and act on.
//!
//! Deliberately not persisted: a defect rate is a claim about recent traffic
//! on this build of everything, and yesterday's rate answering today's
//! question is exactly the staleness ADR 0001 warns about. The loop reacts
//! to what is happening, not to what once happened.
//!
//! That was tried the other way and reverted, so it does not need trying
//! again. Persistence — a `defect_windows` table, exponential decay by
//! wall-clock age, and outright discard of evidence recorded against a
//! different llama.cpp release — was built to let the idle-time tune
//! scheduler carry evidence across restarts. Decay and build scoping existed
//! *only* to answer the staleness objection above; they were the price of
//! persisting at all, not features in their own right.
//!
//! With the scheduler removed, nothing acts on these counts automatically,
//! and sampling defaults now come from the model's own metadata rather than
//! from measured rates. Nobody was left who needed yesterday's numbers, so
//! the whole apparatus went rather than sit dormant. These counters are
//! diagnostic, per-process, and reset on restart — which is the correct
//! lifetime for a claim about what is happening now.

use std::collections::HashMap;
use std::sync::Mutex;

pub use super::defect_counts::{LoopGuardTrip, ModelDefectCounts};

/// Process-lifetime per-model defect counters.
///
/// A synchronous mutex over a small map: every operation is a couple of
/// integer bumps under the lock, on paths that already do far heavier work.
#[derive(Debug, Default)]
pub struct ModelDefectLedger {
    counts: Mutex<HashMap<String, ModelDefectCounts>>,
}

impl ModelDefectLedger {
    /// Create an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one request for `model`.
    pub fn record_request(&self, model: &str) {
        self.with(model, |c| c.requests += 1);
    }

    /// Count one loop-guard intervention for `model`, under the detector that
    /// raised it.
    ///
    /// Since #1052 an intervention is a note *or* a refusal — the default
    /// forwards the request with a note rather than rejecting it.
    ///
    /// Bumps the detector's own count and `loop_guard_trips`, which stays the
    /// sum of the two. Also counts the request itself: a trip outside its own
    /// denominator would overstate every rate computed from these numbers.
    ///
    /// Scoped to the **snapshot**, not the client request. Exactly one of this
    /// and [`Self::record_request`] runs per snapshot recorded, because the
    /// caller branches on whether the snapshot names a detector. A client
    /// request that is noted and then retried after an upstream death records
    /// two snapshots — the second deliberately carries no trip — so it bumps
    /// `requests` twice and `loop_guard_trips` once. That double count of
    /// `requests` predates this and is the retry path's, not the guard's.
    pub fn record_loop_guard_trip(&self, model: &str, which: LoopGuardTrip) {
        self.with(model, |c| {
            c.requests += 1;
            c.loop_guard_trips += 1;
            match which {
                LoopGuardTrip::Loop => c.loop_guard_loops += 1,
                LoopGuardTrip::Stagnation => c.loop_guard_stagnations += 1,
            }
        });
    }

    /// Count one tool-call repair attempt for `model`.
    pub fn record_repair(&self, model: &str, succeeded: bool) {
        self.with(model, |c| {
            c.repairs_attempted += 1;
            if succeeded {
                c.repairs_succeeded += 1;
            }
        });
    }

    /// Count one upstream mid-stream failure for `model`.
    ///
    /// Deliberately does *not* bump `requests`, unlike
    /// [`Self::record_loop_guard_trip`], which counts a request the guard
    /// acted on — refused instead of forwarding, or forwarded with a note —
    /// and so has to count its own denominator either way. A stream error
    /// happens after the request was forwarded and already counted; bumping
    /// here would count the same request twice and deflate every rate.
    pub fn record_stream_error(&self, model: &str) {
        self.with(model, |c| c.stream_errors += 1);
    }

    /// Count one generation cut off at the token ceiling for `model`.
    pub fn record_truncated_generation(&self, model: &str) {
        self.with(model, |c| c.truncated_generations += 1);
    }

    /// Count one turn that produced nothing client-renderable for `model`.
    ///
    /// `reasoning_only` says whether the model produced reasoning and nothing
    /// else. It is counted *within* the empty total, not beside it — the turn
    /// was empty either way, and this records why.
    pub fn record_empty_response(&self, model: &str, reasoning_only: bool) {
        self.with(model, |c| {
            c.empty_responses += 1;
            if reasoning_only {
                c.reasoning_only += 1;
            }
        });
    }

    /// Count one turn where dialect markup reached client-visible output.
    pub fn record_dialect_residue(&self, model: &str) {
        self.with(model, |c| c.dialect_residue += 1);
    }

    /// Count one turn whose tool call could not be validated at all.
    pub fn record_unvalidatable_schema(&self, model: &str) {
        self.with(model, |c| c.unvalidatable_schemas += 1);
    }

    /// Count one turn whose normalization discarded a malformed tool call.
    pub fn record_normalization_error(&self, model: &str) {
        self.with(model, |c| c.normalization_errors += 1);
    }

    /// Count one turn that repeated the call before it and got an equal
    /// result back.
    pub fn record_identical_result_repeat(&self, model: &str) {
        self.with(model, |c| c.identical_result_repeats += 1);
    }

    /// Count one turn the guard would have acted on for repeating and did not,
    /// because the answer had moved. A repeat still inside the allowance is not.
    pub fn record_repeat_rescued(&self, model: &str) {
        self.with(model, |c| c.repeats_rescued += 1);
    }

    /// Record that one turn repeated a batch whose results could not be
    /// compared.
    pub fn record_repeat_not_evaluated(&self, model: &str) {
        self.with(model, |c| c.repeats_not_evaluated += 1);
    }

    /// The current counts for every model that has any.
    #[must_use]
    pub fn snapshot(&self) -> HashMap<String, ModelDefectCounts> {
        self.counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn with(&self, model: &str, update: impl FnOnce(&mut ModelDefectCounts)) {
        let mut counts = self
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update(counts.entry(model.to_owned()).or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_accumulate_per_model() {
        let ledger = ModelDefectLedger::new();
        ledger.record_request("a");
        ledger.record_request("a");
        ledger.record_loop_guard_trip("a", LoopGuardTrip::Loop);
        ledger.record_repair("b", true);
        ledger.record_repair("b", false);

        let snap = ledger.snapshot();
        assert_eq!(snap["a"].requests, 3); // a trip counts its own request
        assert_eq!(snap["a"].loop_guard_trips, 1);
        assert_eq!(snap["b"].repairs_attempted, 2);
        assert_eq!(snap["b"].repairs_succeeded, 1);
    }

    /// A trip is counted under the detector that raised it and in the sum, and
    /// counts its own request once. ADR 0011's first criterion asks about
    /// stagnation alone, which one tally over both detectors could not answer.
    #[test]
    fn a_trip_is_counted_under_its_detector_and_in_the_sum() {
        let ledger = ModelDefectLedger::new();
        ledger.record_loop_guard_trip("a", LoopGuardTrip::Loop);
        ledger.record_loop_guard_trip("a", LoopGuardTrip::Stagnation);
        ledger.record_loop_guard_trip("a", LoopGuardTrip::Stagnation);

        let snap = ledger.snapshot()["a"];
        assert_eq!(snap.loop_guard_loops, 1, "one loop trip");
        assert_eq!(snap.loop_guard_stagnations, 2, "two stagnation trips");
        assert_eq!(snap.loop_guard_trips, 3, "the sum of the two");
        assert_eq!(snap.requests, 3, "each trip counts its own request, once");
    }

    /// A stream error marks an already-forwarded request as having died; it
    /// must not also count a request, or the rate it feeds is deflated by
    /// its own denominator.
    #[test]
    fn a_stream_error_does_not_bump_its_own_denominator() {
        let ledger = ModelDefectLedger::new();
        ledger.record_request("a");
        ledger.record_stream_error("a");

        let snap = ledger.snapshot()["a"];
        assert_eq!(snap.requests, 1, "the turn was counted when forwarded");
        assert_eq!(snap.stream_errors, 1);
    }

    /// `reasoning_only` is a subset of `empty_responses`, not a sibling. A
    /// reader wanting "empty but not reasoning-only" subtracts; one wanting
    /// the empty rate uses the total without having to add two fields.
    #[test]
    fn reasoning_only_turns_are_counted_within_the_empty_total() {
        let ledger = ModelDefectLedger::new();
        ledger.record_empty_response("a", true);
        ledger.record_empty_response("a", false);

        let snap = ledger.snapshot()["a"];
        assert_eq!(snap.empty_responses, 2, "both turns were empty");
        assert_eq!(snap.reasoning_only, 1, "one of them had reasoning");
    }

    /// None of the counted-only instruments touch `requests`. They describe
    /// turns that were already counted when forwarded, so bumping the
    /// denominator here would deflate every rate computed from it.
    #[test]
    fn counted_only_instruments_leave_the_denominator_alone() {
        let ledger = ModelDefectLedger::new();
        ledger.record_request("a");
        ledger.record_truncated_generation("a");
        ledger.record_empty_response("a", false);
        ledger.record_dialect_residue("a");
        ledger.record_unvalidatable_schema("a");
        ledger.record_normalization_error("a");
        ledger.record_stream_error("a");

        let snap = ledger.snapshot()["a"];
        assert_eq!(snap.requests, 1, "one request, however many faults it had");
        assert_eq!(snap.truncated_generations, 1);
        assert_eq!(snap.dialect_residue, 1);
        assert_eq!(snap.unvalidatable_schemas, 1);
        assert_eq!(snap.normalization_errors, 1);
    }
}
