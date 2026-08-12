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
//! zero them). Windowing is the *reader's* job: a reader keeps its own
//! per-model baselines and rates the delta, so two readers can window
//! differently without fighting over a reset button.
//!
//! Since ADR 0006 nothing acts on these automatically. They are diagnosis —
//! what actually fails, per model, for a person to read and act on.
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

/// Cumulative defect counts for one model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ModelDefectCounts {
    /// Requests the proxy forwarded (or would have, but for a guard) for
    /// this model — every rate's denominator.
    pub requests: u64,
    /// Requests the loop/stagnation guard rejected before dispatch.
    pub loop_guard_trips: u64,
    /// Turns whose tool call failed schema validation and was re-issued
    /// with `tool_choice: "required"`.
    pub repairs_attempted: u64,
    /// Of those, the re-issues that produced a conformant call.
    pub repairs_succeeded: u64,
    /// Streaming turns that died on an *upstream* mid-stream failure — an
    /// error event the model server emitted mid-generation, or the byte
    /// stream itself breaking.
    ///
    /// The catastrophic sibling of the repair signal. Both of the counters
    /// above require a model coherent enough to produce structured output:
    /// one counts verbatim repetition, the other a tool call that was
    /// attempted and malformed. A model whose sampling has collapsed
    /// produces neither — it emits output so far outside the expected shape
    /// that the model server kills the stream, and the person's turn simply
    /// fails, invisibly to every other counter here.
    ///
    /// Client disconnects are deliberately not in here: hanging up is a
    /// person's action, not a model defect.
    pub stream_errors: u64,
    /// Turns the model server cut off at the token ceiling
    /// (`finish_reason == "length"`).
    ///
    /// Not a model defect in the same sense as the others — a long answer is
    /// allowed to be long — but a *rising* rate is how a runaway generation
    /// looks before anything else notices, and it is the cheapest evidence
    /// that a context budget is mis-sized.
    pub truncated_generations: u64,
    /// Turns that produced nothing a client can render.
    pub empty_responses: u64,
    /// Of those, the ones that produced reasoning and nothing else.
    ///
    /// Counted inside [`Self::empty_responses`] rather than beside it: the
    /// turn was empty from the client's point of view either way, and the
    /// distinction is *why*. A model stranding its whole answer in
    /// `reasoning_content` is a prompt/template problem; one producing
    /// nothing at all is not.
    pub reasoning_only: u64,
    /// Turns where dialect markup survived normalization into client-visible
    /// output — the drift alarm, per model rather than fleet-wide.
    pub dialect_residue: u64,
    /// Turns whose tool call could not be validated at all, so repair never
    /// had an opinion to act on.
    ///
    /// The blind spot this makes visible: a client whose tools all use
    /// `anyOf` gets zero repair coverage *and*, until now, zero evidence of
    /// that fact. A high rate here means the repair rate below it is
    /// measuring a much smaller slice of traffic than it appears to.
    pub unvalidatable_schemas: u64,
    /// Turns whose normalization discarded a malformed dialect tool call and
    /// surfaced the raw body as visible text instead.
    pub normalization_errors: u64,
}

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

    /// Count one loop-guard rejection for `model`.
    ///
    /// Also counts the request itself: the guard fires *instead of* a
    /// forward, and a trip outside its own denominator would overstate
    /// every rate computed from these numbers.
    pub fn record_loop_guard_trip(&self, model: &str) {
        self.with(model, |c| {
            c.requests += 1;
            c.loop_guard_trips += 1;
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
    /// [`Self::record_loop_guard_trip`]. The guard fires *instead of* a
    /// forward, so it has to count its own denominator; a stream error
    /// happens after the request was forwarded and already counted. Bumping
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

/// The counts accumulated between two snapshots — what a windowing reader
/// actually rates.
#[must_use]
pub const fn delta(current: ModelDefectCounts, baseline: ModelDefectCounts) -> ModelDefectCounts {
    ModelDefectCounts {
        requests: current.requests.saturating_sub(baseline.requests),
        loop_guard_trips: current
            .loop_guard_trips
            .saturating_sub(baseline.loop_guard_trips),
        repairs_attempted: current
            .repairs_attempted
            .saturating_sub(baseline.repairs_attempted),
        repairs_succeeded: current
            .repairs_succeeded
            .saturating_sub(baseline.repairs_succeeded),
        stream_errors: current.stream_errors.saturating_sub(baseline.stream_errors),
        truncated_generations: current
            .truncated_generations
            .saturating_sub(baseline.truncated_generations),
        empty_responses: current
            .empty_responses
            .saturating_sub(baseline.empty_responses),
        reasoning_only: current
            .reasoning_only
            .saturating_sub(baseline.reasoning_only),
        dialect_residue: current
            .dialect_residue
            .saturating_sub(baseline.dialect_residue),
        unvalidatable_schemas: current
            .unvalidatable_schemas
            .saturating_sub(baseline.unvalidatable_schemas),
        normalization_errors: current
            .normalization_errors
            .saturating_sub(baseline.normalization_errors),
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
        ledger.record_loop_guard_trip("a");
        ledger.record_repair("b", true);
        ledger.record_repair("b", false);

        let snap = ledger.snapshot();
        assert_eq!(snap["a"].requests, 3); // a trip counts its own request
        assert_eq!(snap["a"].loop_guard_trips, 1);
        assert_eq!(snap["b"].repairs_attempted, 2);
        assert_eq!(snap["b"].repairs_succeeded, 1);
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

    /// Every new field must window like the old ones, or a reader silently
    /// rates a cumulative total against a windowed denominator.
    #[test]
    fn every_counter_windows_against_its_baseline() {
        let ledger = ModelDefectLedger::new();
        ledger.record_truncated_generation("a");
        ledger.record_empty_response("a", true);
        ledger.record_dialect_residue("a");
        let baseline = ledger.snapshot()["a"];

        ledger.record_truncated_generation("a");
        ledger.record_empty_response("a", true);
        ledger.record_dialect_residue("a");
        ledger.record_unvalidatable_schema("a");
        ledger.record_normalization_error("a");

        let window = delta(ledger.snapshot()["a"], baseline);
        assert_eq!(window.truncated_generations, 1);
        assert_eq!(window.empty_responses, 1);
        assert_eq!(window.reasoning_only, 1);
        assert_eq!(window.dialect_residue, 1);
        assert_eq!(window.unvalidatable_schemas, 1);
        assert_eq!(window.normalization_errors, 1);
    }

    #[test]
    fn a_windowing_reader_rates_the_delta() {
        let ledger = ModelDefectLedger::new();
        for _ in 0..10 {
            ledger.record_request("a");
        }
        let baseline = ledger.snapshot()["a"];
        ledger.record_loop_guard_trip("a");
        ledger.record_request("a");

        let window = delta(ledger.snapshot()["a"], baseline);
        assert_eq!(window.requests, 2);
        assert_eq!(window.loop_guard_trips, 1);
    }
}
