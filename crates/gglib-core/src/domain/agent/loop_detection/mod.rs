#![doc = include_str!("README.md")]
pub(crate) mod results;
pub(crate) mod signature;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "verdict_tests.rs"]
mod verdict_tests;

use super::tool_types::ToolCall;
use crate::ports::AgentError;

pub use signature::batch_signature;

// =============================================================================
// Observation-batch classifier
// =============================================================================

/// Return `true` if **every** call in `calls` is an observation-only tool.
///
/// A tool call is classified as observation-only when its lowercased name
/// satisfies `name.ends_with(pattern) || name.contains(pattern)` for at
/// least one pattern in `patterns`.  Matching is case-insensitive (both
/// sides are lowercased before comparison).
///
/// An empty `patterns` list means no tools are ever classified as
/// observation-only, so the function always returns `false`.
///
/// An empty `calls` slice returns `true` (vacuous truth), but the caller
/// ([`LoopDetector::check`]) is never invoked with an empty batch — both the
/// agent loop and the proxy's history scan skip loop detection when there are
/// no tool calls. That is now load bearing rather than merely tidy: an empty
/// batch would hash to a signature of its own and break the consecutive run.
pub fn is_observation_batch(calls: &[ToolCall], patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    calls.iter().all(|call| {
        let name = call.name.to_lowercase();
        patterns
            .iter()
            .any(|pat| name.ends_with(pat.as_str()) || name.contains(pat.as_str()))
    })
}

// =============================================================================
// LoopDetector
// =============================================================================

/// Stateful guard that detects when the same tool-call batch repeats **back to
/// back and gets the same answer back**.
///
/// Create once per agent run. Call [`LoopDetector::check`] before executing a
/// batch and [`LoopDetector::record_results`] once its answers are known.
///
/// # What counts as a strike
///
/// Counting is run-length, not session-wide: only the current unbroken run of
/// one signature is held, and a batch with a different signature discards it.
/// A session-wide tally made any long conversation terminal — a client replays
/// the whole history every turn, so a batch that recurred often enough
/// anywhere in the session was rejected on every subsequent request for the
/// rest of it.
///
/// Within a run, an occurrence is a strike only when its answers matched the
/// previous occurrence's. The same call with a different answer is progress
/// that happens to look alike — an agent polling a build for output issues an
/// identical batch every time, and run-length counting alone refused it
/// exactly as a session-wide tally did. The verdict could not see what came
/// back; now it can.
///
/// # The ceiling on that
///
/// A changed answer restarts the run, so on its own it would exempt any tool
/// whose output carries a clock, an elapsed time, a progress counter or a
/// random id — `cargo test`'s `finished in 0.31s` is enough. Read-only batches
/// are exempt anyway, because that tier exists on the ground that repeating a
/// call which changes nothing is free. Everything else keeps a ceiling: a
/// batch that changes something may be carried by changing answers only while
/// the run itself stays inside the read-only allowance,
/// `max_observation_steps`. Reusing that number rather than inventing one is
/// deliberate; there is no measurement behind a new one.
///
/// With `max_observation_steps: None` the tier is off: no batch is exempt, the
/// ceiling collapses to `max_strikes`, and since `total >= count` that subsumes
/// the strike count and leaves behaviour exactly as it was before results were
/// read at all — for read-only batches too, which are still *classified* by
/// `observation_tools` and would otherwise have been left unbounded.
///
/// The ceiling is never tighter than `max_strikes`, so lowering the read-only
/// allowance cannot make the guard refuse a mutating batch earlier than its own
/// threshold says.
///
/// # What it still does not catch
///
/// A *cycle* of tool batches, at any period of two or more — A → B → A → B,
/// and equally A → A → B repeating. The run breaks on **signature**, before
/// answers are ever consulted, so reading them changes nothing about this.
/// Separating a cycle from scattered repeats needs a window or a decay rate
/// and there is no measurement behind either number.
///
/// A *quiet* poll, either. Sixteen identical answers in a row to a read-only
/// batch is still a loop by this detector's definition, so an agent watching a
/// compile that prints nothing for two minutes is still refused at the
/// observation ceiling. Result-awareness helps only once the output moves.
///
/// [`super::StagnationDetector`] keeps its session-wide counting and catches
/// an oscillating session *only if the model also repeats its prose* — and a
/// tool-call-only turn carries `content: null`, which that detector ignores by
/// design. What observes the rest is the proxy's ledger, which is a reading
/// for a person and not a verdict.
#[derive(Debug, Default)]
pub struct LoopDetector {
    /// The current unbroken run, or `None` until the first batch arrives.
    run: Option<Run>,
}

/// One unbroken run of a single batch signature.
#[derive(Debug)]
struct Run {
    /// The signature every occurrence in this run shares.
    signature: String,
    /// Occurrences since the answers last changed. This is what the threshold
    /// is compared against.
    count: usize,
    /// Occurrences in this run, never reset by a changed answer. This is what
    /// the read-only allowance is compared against, for a batch that is not
    /// read-only.
    total: usize,
    /// The answers recorded for the most recent occurrence. `None` means they
    /// could not be joined — or have not been recorded yet, which is the same
    /// thing to a verdict that has nothing to compare.
    last_answers: Option<u64>,
}

/// Names the batch [`LoopDetector::check`] just counted.
///
/// [`LoopDetector::record_results`] takes one so it cannot be handed the wrong
/// batch. The answers it records belong to the batch that *just ran*, and the
/// comparison at the next `check` is against those; passing the previous
/// batch's instead would invert the measurement silently, which is a mistake
/// this codebase has made once already with a global id map and caught only in
/// review.
/// Consumed by `record_results` and deliberately not `Clone`: recording one
/// batch twice would let a second, different answer rescue a run that never
/// changed its answer, and no call site has a reason to do it. The type is what
/// stops that rather than a rule in prose.
#[derive(Debug)]
#[must_use = "the batch that was checked must have its answers recorded, or the verdict cannot read them"]
pub struct BatchRecord {
    signature: String,
}

/// What one occurrence turned out to be, once its answers were known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatOutcome {
    /// First occurrence in this run, or answers that could not be compared on
    /// one side or the other. Not evidence of progress, and not evidence
    /// against it.
    NotComparable,
    /// The same batch got the same answer back. The run stands.
    AnswerRepeated,
    /// The same batch got a different answer. This occurrence was progress, so
    /// the run starts again from it.
    AnswerChanged,
    /// The record does not name the current run — the run was already broken
    /// by a different batch before its answers arrived. Nothing is recorded.
    NotTheCurrentRun,
}

impl LoopDetector {
    /// Count this batch and error if it has now repeated too often.
    ///
    /// Selects the effective threshold by batch classification: if every call
    /// matches an observation pattern (via [`is_observation_batch`]),
    /// `max_observation_steps` is used, falling back to `max_strikes` when
    /// `None`. Otherwise `max_strikes`.
    ///
    /// The count is incremented **before** the comparison, so `max_strikes = 2`
    /// allows two identical batches and errors on the third, and
    /// `max_strikes = 0` rejects the very first occurrence.
    ///
    /// A batch with a different signature resets the run to one, and that is
    /// the only thing that resets it. Both call sites skip this method when the
    /// batch is empty, so a prose answer, a `role: "tool"` result and a user
    /// interjection are all transparent to a run. That is load bearing rather
    /// than incidental: every real tool call is answered by a result message
    /// before the next one, so a run that those could break would never reach
    /// two.
    ///
    /// # Errors
    ///
    /// [`AgentError::LoopDetected`] when the run has passed its threshold, or
    /// when a batch that is not read-only has been carried past the read-only
    /// allowance by changing answers. The two are not distinguished in the
    /// error: the remedy is identical, and the variant is mirrored into the
    /// proxy's 400 body.
    pub fn check(
        &mut self,
        calls: &[ToolCall],
        max_strikes: usize,
        observation_tools: &[String],
        max_observation_steps: Option<usize>,
    ) -> Result<BatchRecord, AgentError> {
        let observation = is_observation_batch(calls, observation_tools);
        let observation_max = max_observation_steps.unwrap_or(max_strikes);
        let effective_max = if observation {
            observation_max
        } else {
            max_strikes
        };
        // Exempt from the ceiling only when the tier is actually configured.
        // `is_observation_batch` reads `observation_tools`, which is a separate
        // field: with `max_observation_steps: None` a read-only batch is still
        // *classified*, so waiving the ceiling on classification alone left it
        // with no bound at all and a moving answer could carry it forever.
        let exempt = observation && max_observation_steps.is_some();
        // Never tighter than the strike threshold. `total >= count` always, so
        // an allowance below `max_strikes` would become the strike threshold
        // for mutating batches and refuse them *earlier* than configured —
        // lowering the read-only allowance must not tighten the guard for
        // tools it does not classify.
        let ceiling = observation_max.max(max_strikes);
        let sig = batch_signature(calls);
        let (count, total) = match &mut self.run {
            Some(run) if run.signature == sig => {
                run.count += 1;
                run.total += 1;
                (run.count, run.total)
            }
            slot => {
                *slot = Some(Run {
                    signature: sig.clone(),
                    count: 1,
                    total: 1,
                    last_answers: None,
                });
                (1, 1)
            }
        };
        if count > effective_max || (!exempt && total > ceiling) {
            return Err(AgentError::LoopDetected { signature: sig });
        }
        Ok(BatchRecord { signature: sig })
    }

    /// Record what the batch named by `record` got back.
    ///
    /// Called once the answers exist, which on the agent path is *after* the
    /// batch executes — the reason the verdict and the recording are separate
    /// calls at all. The proxy calls both together, since it reads a completed
    /// transcript.
    ///
    /// `answers` is `None` when the batch was unanswered or only partly
    /// answered. Unknown answers never rescue a run: an answer nobody can read
    /// is not evidence of progress, and treating it as such would let any
    /// client that omits `id` on replayed calls switch the guard off. It is
    /// also what makes a detector that is never told anything behave exactly as
    /// it did before it could be.
    pub fn record_results(&mut self, record: BatchRecord, answers: Option<u64>) -> RepeatOutcome {
        // Destructured rather than read through: taking the record by value is
        // what stops one batch being recorded twice, and clippy's
        // `needless_pass_by_value` fires on a by-value argument whose body only
        // *reads* a field. Its suggested remedy — take `&BatchRecord` — would
        // reinstate exactly the hole this signature closes, so the argument is
        // consumed here instead of silenced.
        let BatchRecord { signature } = record;
        let Some(run) = self.run.as_mut() else {
            return RepeatOutcome::NotTheCurrentRun;
        };
        if run.signature != signature {
            return RepeatOutcome::NotTheCurrentRun;
        }
        let outcome = match (run.last_answers, answers) {
            (Some(previous), Some(now)) if previous != now => {
                // This occurrence produced something new, so the run of
                // identical answers starts here. `total` is untouched: it is
                // the ceiling on how far this can be repeated.
                run.count = 1;
                RepeatOutcome::AnswerChanged
            }
            (Some(_), Some(_)) => RepeatOutcome::AnswerRepeated,
            _ => RepeatOutcome::NotComparable,
        };
        run.last_answers = answers;
        outcome
    }
}
