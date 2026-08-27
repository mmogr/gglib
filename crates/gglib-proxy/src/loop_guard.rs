//! Pre-dispatch loop/stagnation guard for `/v1/chat/completions`.
//!
//! The built-in agent loop (`gglib-agent`) aborts a run when the model
//! repeats the same tool-call batch back to back and keeps getting the same
//! answer back, or repeats the same prose too often in a short span —
//! but external
//! agentic clients (Cline, Roo Code, Copilot BYOK) run their own loop
//! client-side, where those guards never execute.  A model looping in such a
//! session burns a model swap plus a full generation per stuck turn, and
//! nothing in gglib notices.
//!
//! This module closes that gap **statelessly**: agentic clients replay the
//! full conversation on every request, so the guard reconstructs the
//! detectors' state fresh per request by walking the incoming `messages[]`
//! history through the *same* [`LoopDetector`] and [`StagnationDetector`]
//! the agent path uses (`gglib_core::domain::agent`).  Parity is by
//! construction — there is one detector implementation, not two — and no
//! per-session store, TTL, or eviction is needed.
//!
//! Detection is deliberately **pre-admission**: a tripped guard returns a
//! clean HTTP 400 before any catalog/admission/model-swap cost.  This catches
//! a loop one turn after the agent path's per-iteration check would (the
//! history at turn N shows responses 1..N-1), which caps a runaway session at
//! threshold+1 turns — accepted for a guard whose job is "fail fast and
//! loud", not mid-stream intervention.
//!
//! Parse policy is **fail-open**: this guard is protection, not validation.
//! An unparseable body yields [`LoopGuardVerdict::Pass`] (routing already
//! rejected genuinely invalid JSON), and a tool call whose `arguments` string
//! is not valid JSON is hashed as the raw string rather than erroring — a
//! client sending consistently malformed arguments still gets loop
//! protection and never gets a parse-driven rejection.
//!
//! Fail-open is also the sharpest edge here, because it is *silent*: a body
//! that fails to deserialize switches the guard off for that request, and a
//! replayed history brings the offending message back on every request after
//! it. So the surface that can fail is kept as small as the wire format allows
//! — see [`wire`], where every field a client controls is a
//! `serde_json::Value` and only `messages` itself is typed.

use std::collections::HashMap;

use gglib_core::domain::agent::{
    AgentConfig, LoopDetector, RepeatOutcome, StagnationDetector, batch_signature,
};
use gglib_core::ports::AgentError;
use gglib_core::{DEFAULT_MAX_STAGNATION_STEPS, Settings, ToolCall};

/// The permissive view of the incoming history.
///
/// A child module rather than a section of this file: the reason every field
/// there is a `serde_json::Value` is a page of argument, and it belongs beside
/// the types it governs rather than in the middle of the scan.
#[path = "loop_guard_wire.rs"]
mod wire;

use wire::HistoryEnvelope;

/// Reached by `loop_guard_tests.rs` through `use super::*`.
///
/// The scan no longer names these — the results join moved to [`wire`] — but
/// the tests that pin its behaviour still build answers by hand. Imported here
/// rather than in the test file because that file is frozen at its current size
/// by the complexity ratchet.
#[cfg(test)]
use {
    gglib_core::domain::agent::{batch_results_hash, hash_result_content},
    serde_json::Value,
};

// =============================================================================
// Configuration
// =============================================================================

/// Thresholds for one request's history scan, resolved from the per-request
/// settings snapshot.
///
/// Loop and observation thresholds come from [`AgentConfig::default`] — the
/// same values the agent path runs with — and the stagnation threshold from
/// the shared persisted `max_stagnation_steps` setting, so the two paths
/// cannot drift.
#[derive(Debug, Clone)]
pub(crate) struct LoopGuardConfig {
    max_repeated_batch_steps: usize,
    max_stagnation_steps: usize,
    observation_tools: Vec<String>,
    max_observation_steps: Option<usize>,
}

impl LoopGuardConfig {
    /// Resolve the guard configuration from a settings snapshot.
    ///
    /// Returns `None` when the guard is disabled — either explicitly
    /// (`proxy_loop_detection = Some(false)`) or because the shared agent
    /// defaults disable loop detection entirely.
    pub(crate) fn from_settings(settings: &Settings) -> Option<Self> {
        if settings.proxy_loop_detection == Some(false) {
            return None;
        }
        let defaults = AgentConfig::default();
        Some(Self {
            max_repeated_batch_steps: defaults.max_repeated_batch_steps?,
            max_stagnation_steps: settings
                .max_stagnation_steps
                .map_or(DEFAULT_MAX_STAGNATION_STEPS, |v| v as usize),
            observation_tools: defaults.observation_tools,
            max_observation_steps: defaults.max_observation_steps,
        })
    }
}

// =============================================================================
// Verdict
// =============================================================================

/// Outcome of scanning one request's replayed history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoopGuardVerdict {
    /// No guard tripped — forward the request.
    Pass,
    /// The same tool-call batch signature repeats back to back and keeps
    /// getting the same answer back, beyond the threshold. Occurrences
    /// separated by other work do not count, and neither does a repeat whose
    /// answer changed.
    ///
    /// Also raised when a batch that is not read-only has been carried past
    /// the read-only allowance by changing answers. The two are not
    /// distinguished: the remedy is the same.
    LoopDetected {
        /// The repeated batch signature (`name:hash|name:hash…`).
        signature: String,
    },
    /// The same assistant text repeats beyond the threshold.
    StagnationDetected {
        /// Occurrences seen, including the one that tripped.
        count: usize,
        /// The configured threshold.
        max_steps: usize,
    },
}

// =============================================================================
// Scan outcome
// =============================================================================

/// What one history scan concluded.
///
/// The verdict is the decision; the bits beside it are readings for a person.
/// A batch that repeats *under* the threshold never reaches a verdict at all,
/// and whether its results were identical is the difference between a model
/// stuck in a loop and a model making progress that happens to look alike.
///
/// The verdict now reads that difference too — ADR 0010 — but not through
/// these bits. Two of them are computed from a session-wide map keyed by
/// signature, while the detector compares within the current run, so they
/// answer neighbouring questions rather than the same one. The third is the
/// detector's own outcome, and exists so the change ADR 0010 made has a
/// reading of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanOutcome {
    /// Whether to forward, and if not, why.
    pub(crate) verdict: LoopGuardVerdict,
    /// Whether this request's **newest** tool-call batch repeated the batch
    /// before it and got an equal result back.
    ///
    /// The comparison is against the *preceding* occurrence of that
    /// signature, not any earlier one: `A(r1), A(r2), A(r1)` reports false at
    /// the third A, because the model did get a different answer last time.
    ///
    /// Deliberately one bit per request rather than a count over the history.
    /// A client replays the whole conversation every turn, so a running total
    /// would re-count the same event on every subsequent request and grow
    /// quadratically in conversation length — a session with three stuck
    /// repeats over fifty turns would report about a hundred. The question
    /// worth answering is "did *this* turn repeat itself", once per turn.
    pub(crate) identical_result_repeat: bool,
    /// Whether the newest batch repeated the one before it but the results
    /// could **not** be compared.
    ///
    /// The difference between "no repeat happened" and "a repeat happened and
    /// gglib could not tell" — states that would otherwise both read as
    /// `identical_result_repeat: false`. The join fails when a client omits
    /// `id` on replayed tool calls (which the wire types above exist to
    /// tolerate), when results are not contiguous after the assistant turn,
    /// or when any call in a parallel batch went unanswered.
    ///
    /// Recorded because a decision rests on the other field reading near zero
    /// — see ADR 0006's 2026-08-26 postscript. A near-zero count is only
    /// evidence that repeats are rare if the question was actually being
    /// asked; without this, an instrument that never joined anything is
    /// indistinguishable from a clean fleet, and would cancel the work it
    /// exists to justify.
    pub(crate) repeat_not_evaluated: bool,
    /// Whether the newest batch repeated, got a **different** answer, and was
    /// let through on that basis.
    ///
    /// Read from the detector's run-scoped outcome rather than the map the two
    /// bits above use, so it answers a different question: not "did this
    /// conversation repeat itself" but "did the guard decline to act because
    /// the answer moved". ADR 0010's kill criteria read it against
    /// `identical_result_repeat`.
    pub(crate) repeat_rescued: bool,
}

// =============================================================================
// History scan
// =============================================================================

/// Walk the request's `messages[]` history through fresh detectors.
///
/// Mirrors `gglib-agent`'s per-iteration guard step exactly — including the
/// `check`-then-`record_results` pair, which this path can do in one place
/// because it reads a transcript that already has the answers: stagnation
/// records every assistant message's text (the detector itself skips empty
/// text), and the loop detector only sees non-empty tool-call batches.
///
/// That second half decides what breaks a loop run, now that the detector
/// counts consecutively. Only an assistant turn carrying a *different* batch
/// does. A `role: "tool"` result, a prose answer and a user interjection are
/// all transparent to it — which is load bearing rather than incidental, since
/// every real tool call is answered by a result message before the next one,
/// so a run those could break would never reach two.
///
/// Fail-open: an unparseable body returns [`LoopGuardVerdict::Pass`].
///
/// One pass. A tool result belongs to the assistant turn it immediately
/// follows, so the join is positional rather than a global index by
/// `tool_call_id` — gglib mints those ids itself for dialect models
/// (`DelimitedToolCallParser` restarts at zero on every response), so
/// `call_qwen_0` recurs on every turn of a replayed conversation and a global
/// map would resolve every occurrence of a batch to the same result.
pub(crate) fn scan_history(body: &[u8], cfg: &LoopGuardConfig) -> ScanOutcome {
    let Ok(envelope) = serde_json::from_slice::<HistoryEnvelope>(body) else {
        return ScanOutcome {
            verdict: LoopGuardVerdict::Pass,
            identical_result_repeat: false,
            repeat_not_evaluated: false,
            repeat_rescued: false,
        };
    };

    let mut stagnation = StagnationDetector::default();
    let mut loops = LoopDetector::default();
    // Batch signature -> the results hash from the last time that exact batch
    // appeared. Absent from the map means "first time"; a `None` value means
    // the batch went unanswered, which is not evidence of anything.
    let mut previous: HashMap<String, Option<u64>> = HashMap::new();
    // Overwritten by each batch, so what survives describes the newest one.
    // See `ScanOutcome::identical_result_repeat` for why this is not a tally.
    let mut identical_result_repeat = false;
    let mut repeat_not_evaluated = false;
    let mut repeat_rescued = false;

    for (i, msg) in envelope.messages.iter().enumerate() {
        match msg.role.as_str() {
            // A batch's own results are part of that turn, not the end of it.
            Some("tool") => continue,
            Some("assistant") => {}
            // Anything else — a user interjecting mid-turn, a system message —
            // ends the observation, exactly as a prose answer does below. The
            // bits describe the batch the next generation follows, and the
            // request that carried that batch has already reported it.
            //
            // It also ends both detectors' state, which is what the agent path
            // gets for free: `AgentLoop::run` is invoked once per user message
            // and builds a fresh `Guards`, so a user turn resets everything
            // there. One detector walking a whole replayed conversation has to
            // be told, and without this the two paths refuse at different
            // turns while the docs above promise parity by construction.
            //
            // This is also the only way out of a transcript that already
            // contains a trip. Both detectors judge the replay from its
            // beginning, so an early run of identical batches — or of
            // identical prose — would otherwise refuse every later request
            // forever, however much good work followed.
            _ => {
                identical_result_repeat = false;
                repeat_not_evaluated = false;
                repeat_rescued = false;
                loops.break_run();
                stagnation.clear();
                continue;
            }
        }

        // Computed before the guards, so the bits describe *this* message even
        // when stagnation rejects it. The loop detector reads `results` below,
        // but not these bits: it compares within its own run, and these are a
        // session-wide reading.
        let calls: Vec<ToolCall> = wire::domain_calls(&msg.tool_calls);
        // Hoisted so the verdict below can read it. The bits are still computed
        // in the branch, and still before the guards run.
        let mut results = None;
        if calls.is_empty() {
            // A prose turn ends the observation. Without this the bits stay
            // set from whatever batch came last and are re-reported on every
            // subsequent request — ask, tools, prose answer, follow-up is the
            // ordinary shape of a chat session, so the inflation is unbounded.
            identical_result_repeat = false;
            repeat_not_evaluated = false;
            repeat_rescued = false;
        } else {
            results = wire::turn_results_hash(&calls, &envelope.messages[i + 1..]);
            let seen_before = previous.insert(batch_signature(&calls), results);
            identical_result_repeat =
                matches!(seen_before, Some(Some(seen)) if Some(seen) == results);
            // A repeat gglib could not evaluate is not a repeat that did not
            // happen, and the two must not share a reading.
            repeat_not_evaluated =
                matches!(seen_before, Some(prior) if prior.is_none() || results.is_none());
        }

        if let Err(e) = stagnation.record(
            &wire::extract_text(&msg.content),
            !calls.is_empty(),
            cfg.max_stagnation_steps,
        ) {
            return ScanOutcome {
                verdict: verdict(e),
                identical_result_repeat,
                repeat_not_evaluated,
                // Not carried over from the previous turn. The two bits above
                // are computed before the guards and so describe *this*
                // message; this one is only known after `check`, and a guard
                // that rejected this batch did not decline to act on it.
                // Shipping the last turn's value would count one rescue again
                // on every replay of a 400'd body, and inflate precisely the
                // ratio ADR 0010's first kill criterion reads.
                //
                // Since a turn that called a tool is no longer recorded, this
                // arm is now reachable only on a prose turn — where the branch
                // above has already cleared all three. Kept explicit rather
                // than collapsed to that observation: the invariant lives in
                // another crate, and a `false` costs nothing to state twice.
                repeat_rescued: false,
            };
        }
        if !calls.is_empty() {
            match loops.check(
                &calls,
                cfg.max_repeated_batch_steps,
                &cfg.observation_tools,
                cfg.max_observation_steps,
            ) {
                Err(e) => {
                    return ScanOutcome {
                        verdict: verdict(e),
                        identical_result_repeat,
                        repeat_not_evaluated,
                        // See the stagnation arm above: a batch the guard
                        // refused was not one it let through.
                        repeat_rescued: false,
                    };
                }
                // Recorded immediately, because this scan reads a transcript
                // that already has the answers. The agent path cannot do this
                // in one step — its batch has not run yet — which is the whole
                // reason `check` and `record_results` are two calls.
                Ok(record) => {
                    repeat_rescued =
                        loops.record_results(record, results) == RepeatOutcome::AnswerChanged;
                }
            }
        }
    }

    ScanOutcome {
        verdict: LoopGuardVerdict::Pass,
        identical_result_repeat,
        repeat_not_evaluated,
        repeat_rescued,
    }
}

/// Map a detector error onto the guard's verdict.
fn verdict(e: AgentError) -> LoopGuardVerdict {
    match e {
        AgentError::LoopDetected { signature } => LoopGuardVerdict::LoopDetected { signature },
        AgentError::StagnationDetected {
            count, max_steps, ..
        } => LoopGuardVerdict::StagnationDetected { count, max_steps },
        // The detectors return no other variant; treat anything unexpected as
        // a pass rather than inventing a rejection (fail-open).
        _ => LoopGuardVerdict::Pass,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "loop_guard_tests.rs"]
mod tests;

/// Stagnation cases. Its own module because `loop_guard_tests.rs` is frozen at
/// its current size by the complexity ratchet.
#[cfg(test)]
#[path = "loop_guard_stagnation_tests.rs"]
mod stagnation_tests;

/// What a user turn resets, on both detectors.
#[cfg(test)]
#[path = "loop_guard_user_turn_tests.rs"]
mod user_turn_tests;
