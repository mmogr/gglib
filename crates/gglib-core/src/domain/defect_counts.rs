//! The per-model defect counts — the data half of [`super::defects`].
//!
//! [`super::defects`] draws a line between a writer that never interprets and a
//! reader that never guesses. This is the reader's side of it: one plain struct
//! of cumulative counts, serialised to the dashboard as it stands. Nearly all of
//! this file is field documentation, because what a counter does *not* count is
//! what a person reading a number needs to know, and it has to be said where the
//! field is declared or it is said nowhere. Beside it, [`LoopGuardTrip`]: the
//! one fact a writer hands over with a count, which detector raised a trip.
//!
//! The ledger that bumps these lives in [`super::defects`], which re-exports
//! both, so `gglib_core::domain::defects::ModelDefectCounts` still names it.

/// Which of the loop guard's two detectors raised a trip.
///
/// The guard is two detectors behind one verdict, and until this existed their
/// trips went into one number, so nobody could ask whether *stagnation*
/// rejections had become rare, which is the question that decides whether
/// `StagnationDetector` survives (ADR 0011's first kill criterion, #947).
///
/// It says which detector, and nothing about which path. Only the proxy's
/// pre-dispatch scan records a trip at all: the agent loop runs the same two
/// detectors and its trips reach no counter (#1091).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum LoopGuardTrip {
    /// The same tool-call batch repeated back to back and kept getting the
    /// same answer.
    ///
    /// Also a batch that is not read-only carried past the read-only allowance
    /// by changing answers. The guard's verdict does not tell those two apart,
    /// because the remedy is the same, so neither does this.
    Loop,
    /// The same assistant text repeated beyond the threshold.
    Stagnation,
}

/// Cumulative defect counts for one model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelDefectCounts {
    /// Requests the proxy forwarded (or would have, but for a guard) for
    /// this model — every rate's denominator.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub requests: u64,
    /// Requests the loop/stagnation guard rejected before dispatch.
    ///
    /// The sum of the two counts below, kept because it is the row people
    /// already read and the one an older dashboard knows. Adding all three
    /// double-counts.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub loop_guard_trips: u64,
    /// Of those, the ones [`LoopGuardTrip::Loop`] raised.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub loop_guard_loops: u64,
    /// Of those, the ones [`LoopGuardTrip::Stagnation`] raised.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub loop_guard_stagnations: u64,
    /// Turns whose tool call failed schema validation and was re-issued,
    /// with `tool_choice: "required"` or as a second draw under gglib's grammar.
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
    /// Turns the loop guard would have refused for repeating, and did not,
    /// because the answer had moved. A repeat inside the allowance is not one.
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
