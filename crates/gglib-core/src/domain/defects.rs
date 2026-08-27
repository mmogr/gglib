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

/// Cumulative defect counts for one model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelDefectCounts {
    /// Requests the proxy forwarded (or would have, but for a guard) for
    /// this model — every rate's denominator.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub requests: u64,
    /// Requests the loop/stagnation guard rejected before dispatch.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub loop_guard_trips: u64,
    /// Turns whose tool call failed schema validation and was re-issued
    /// with `tool_choice: "required"`.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub repairs_attempted: u64,
    /// Of those, the re-issues that produced a conformant call.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub stream_errors: u64,
    /// Turns the model server cut off at the token ceiling
    /// (`finish_reason == "length"`).
    ///
    /// Not a model defect in the same sense as the others — a long answer is
    /// allowed to be long — but a *rising* rate is how a runaway generation
    /// looks before anything else notices, and it is the cheapest evidence
    /// that a context budget is mis-sized.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub truncated_generations: u64,
    /// Turns that produced nothing a client can render.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub empty_responses: u64,
    /// Of those, the ones that produced reasoning and nothing else.
    ///
    /// Counted inside [`Self::empty_responses`] rather than beside it: the
    /// turn was empty from the client's point of view either way, and the
    /// distinction is *why*. A model stranding its whole answer in
    /// `reasoning_content` is a prompt/template problem; one producing
    /// nothing at all is not.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub reasoning_only: u64,
    /// Turns where dialect markup survived normalization into client-visible
    /// output — the drift alarm, per model rather than fleet-wide.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub dialect_residue: u64,
    /// Turns whose tool call could not be validated at all, so repair never
    /// had an opinion to act on.
    ///
    /// The blind spot this makes visible: a client whose tools all use
    /// `anyOf` gets zero repair coverage *and*, until now, zero evidence of
    /// that fact. A high rate here means the repair rate below it is
    /// measuring a much smaller slice of traffic than it appears to.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub unvalidatable_schemas: u64,
    /// Turns whose normalization discarded a malformed dialect tool call and
    /// surfaced the raw body as visible text instead.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub normalization_errors: u64,
    /// Turns whose newest tool-call batch repeated the batch before it and
    /// got an equal result back.
    ///
    /// The comparison is against the *preceding* occurrence of that signature,
    /// not any earlier one: a call that returned A, then B, then A again is
    /// not counted, because the model did get a different answer last time.
    ///
    /// The odd one out, deliberately. Every counter above measures a gglib
    /// organ firing or a defect in the shape of the model's own output. This
    /// one measures a condition in the *conversation*: the model asked for
    /// the same thing twice and the environment answered the same way twice,
    /// which is the only evidence available that a repeat was genuinely
    /// stuck rather than progress that happens to look alike.
    ///
    /// One increment per turn, like every counter above it — not a tally over
    /// the replayed history. A client resends the whole conversation each
    /// turn, so counting history-wide would re-count the same event on every
    /// later request and grow with the square of session length.
    ///
    /// "Equal" means equal after hashing the result's `content` as it
    /// arrived, per turn. Bounded to the calls the batch actually made, and
    /// only when every one of them was answered.
    ///
    /// Counted whether or not the guard trips — a repeat under the threshold
    /// is exactly the case a verdict cannot see. Nothing acts on it: it
    /// exists to answer whether a corrective arm on the input plane would
    /// ever have a trigger, before one is built.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub identical_result_repeats: u64,
    /// Turns whose newest tool-call batch repeated the batch before it but
    /// whose results could **not** be compared.
    ///
    /// The denominator for the counter above, and the reason a zero there can
    /// be read at all. A repeat gglib could not evaluate is not a repeat that
    /// did not happen: without this, an instrument that never managed to join
    /// a single result would look exactly like a fleet with nothing wrong.
    ///
    /// Bumps when a client omits `id` on replayed tool calls, when results are
    /// not contiguous after the assistant turn, or when a parallel batch went
    /// partly unanswered.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub repeats_not_evaluated: u64,
    /// Turns where a repeated batch got a **different** answer, and the loop
    /// guard let it through on that basis.
    ///
    /// Unlike the two above, this is not a fact about the conversation — it is
    /// a fact about gglib's own reflex, which is what the ledger was chartered
    /// for before ADR 0006 had to widen it. It reads the detector's run-scoped
    /// outcome, not the session-wide map those two are computed from, so it is
    /// a third instrument rather than a third view of one.
    ///
    /// It exists because ADR 0010 promoted the results join from an
    /// observation to a policy input, and a kill criterion nobody can read is
    /// not a kill criterion. If this dwarfs `identical_result_repeats` in real
    /// use, the join is being defeated by output that carries a clock rather
    /// than measuring progress, and the rescue wants narrowing or removing.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub repeats_rescued: u64,
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

    /// Count one turn that repeated the call before it and got an equal
    /// result back.
    pub fn record_identical_result_repeat(&self, model: &str) {
        self.with(model, |c| c.identical_result_repeats += 1);
    }

    /// Count one turn where a repeated batch got a different answer and the
    /// loop guard declined to act on that basis.
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
}
