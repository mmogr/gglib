//! Pre-dispatch loop/stagnation guard for `/v1/chat/completions`.
//!
//! The built-in agent loop (`gglib-agent`) aborts a run when the model
//! repeats the same tool-call batch back to back, or the same response text
//! anywhere in the session — but external
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

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::Deserialize;
use serde_json::Value;

use gglib_core::domain::agent::{AgentConfig, LoopDetector, StagnationDetector, batch_signature};
use gglib_core::ports::AgentError;
use gglib_core::{DEFAULT_MAX_STAGNATION_STEPS, Settings, ToolCall};

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
    /// The same tool-call batch signature repeats back to back, beyond the
    /// threshold. Occurrences separated by other work do not count.
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
// Permissive wire types
// =============================================================================
//
// Deliberately NOT `crate::models::ToolCall`: a client replaying history may
// omit `id` or `type` on old messages, and a guard must never 400 a request
// because of a shape quirk in content it is only inspecting.  Every field
// defaults.

#[derive(Deserialize)]
struct HistoryEnvelope {
    #[serde(default)]
    messages: Vec<HistoryMessage>,
}

#[derive(Deserialize)]
struct HistoryMessage {
    #[serde(default)]
    role: Value,
    /// Assistant text is read via [`extract_text`]; on a `role: "tool"`
    /// message the whole value is hashed by [`hash_content`] instead.
    #[serde(default)]
    content: Value,
    #[serde(default)]
    tool_calls: Vec<WireToolCall>,
    /// Present on `role: "tool"` messages: the id of the call this is the
    /// result of.  The join key between the model's request and the
    /// environment's answer, and the reason this struct is no longer
    /// assistant-only.
    ///
    /// `Value`, not `Option<String>`, for the reason stated above every field
    /// here: a typed field rejects a body whose shape is merely odd, and a
    /// failed deserialize takes the *whole envelope* with it — silently
    /// disabling the guard for that request. Read through `as_str`.
    #[serde(default)]
    tool_call_id: Value,
}

#[derive(Deserialize)]
struct WireToolCall {
    #[serde(default)]
    id: Value,
    #[serde(default)]
    function: WireFunction,
}

#[derive(Deserialize, Default)]
struct WireFunction {
    #[serde(default)]
    name: Value,
    /// OpenAI wire format says a JSON-encoded *string*, but models and
    /// bridges routinely emit a bare object here, and `ToolCall::arguments`
    /// in the domain is a `Value` for that reason. Typed as `String` this
    /// failed the whole envelope and switched the guard off for the request.
    #[serde(default)]
    arguments: Value,
}

// =============================================================================
// Scan outcome
// =============================================================================

/// What one history scan concluded.
///
/// The verdict is the decision; the bits beside it are observations the
/// verdict cannot express. A batch that repeats *under* the threshold never
/// reaches a verdict at all, and whether its results were identical is the
/// difference between a model stuck in a loop and a model making progress
/// that happens to look alike — so it is measured separately and acted on by
/// nothing.
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
}

// =============================================================================
// History scan
// =============================================================================

/// Walk the request's `messages[]` history through fresh detectors.
///
/// Mirrors `gglib-agent`'s per-iteration `Guards::check` exactly: stagnation
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

    for (i, msg) in envelope.messages.iter().enumerate() {
        match msg.role.as_str() {
            // A batch's own results are part of that turn, not the end of it.
            Some("tool") => continue,
            Some("assistant") => {}
            // Anything else — a user interjecting mid-turn, a system message —
            // ends the observation, exactly as a prose answer does below. The
            // bits describe the batch the next generation follows, and the
            // request that carried that batch has already reported it.
            _ => {
                identical_result_repeat = false;
                repeat_not_evaluated = false;
                continue;
            }
        }

        // Computed before the guards, so the bits describe *this* message even
        // when stagnation rejects it. Neither detector can see them: the
        // observation has no effect on the verdicts below.
        let calls: Vec<ToolCall> = msg.tool_calls.iter().map(to_domain_call).collect();
        if calls.is_empty() {
            // A prose turn ends the observation. Without this the bits stay
            // set from whatever batch came last and are re-reported on every
            // subsequent request — ask, tools, prose answer, follow-up is the
            // ordinary shape of a chat session, so the inflation is unbounded.
            identical_result_repeat = false;
            repeat_not_evaluated = false;
        } else {
            let results = turn_results_hash(&calls, &envelope.messages[i + 1..]);
            let seen_before = previous.insert(batch_signature(&calls), results);
            identical_result_repeat =
                matches!(seen_before, Some(Some(seen)) if Some(seen) == results);
            // A repeat gglib could not evaluate is not a repeat that did not
            // happen, and the two must not share a reading.
            repeat_not_evaluated =
                matches!(seen_before, Some(prior) if prior.is_none() || results.is_none());
        }

        if let Err(e) = stagnation.record(&extract_text(&msg.content), cfg.max_stagnation_steps) {
            return ScanOutcome {
                verdict: verdict(e),
                identical_result_repeat,
                repeat_not_evaluated,
            };
        }
        if !calls.is_empty()
            && let Err(e) = loops.check(
                &calls,
                cfg.max_repeated_batch_steps,
                &cfg.observation_tools,
                cfg.max_observation_steps,
            )
        {
            return ScanOutcome {
                verdict: verdict(e),
                identical_result_repeat,
                repeat_not_evaluated,
            };
        }
    }

    ScanOutcome {
        verdict: LoopGuardVerdict::Pass,
        identical_result_repeat,
        repeat_not_evaluated,
    }
}

/// Hash the results answering one assistant turn's tool calls.
///
/// `rest` is the history *after* that assistant message; the answers are the
/// contiguous run of result messages at its head, which bounds the join to
/// this turn and makes repeated synthetic ids harmless.
///
/// Each call is paired with its own answer before sorting. Sorting bare result
/// hashes would meet the ordering goal — [`batch_signature`] sorts too, so the
/// same parallel batch re-emitted in a different order must still match — but
/// it severs which call produced which result, and a two-call batch whose
/// answers swapped would compare equal.
///
/// The pair key renders `arguments` rather than hashing it structurally, which
/// relies on `serde_json::Value` being a `BTreeMap`. Enabling that crate's
/// `preserve_order` feature would make the rendering insertion-ordered and this
/// join would quietly start under-reporting.
///
/// `None` when any call is unanswered — a partially-answered batch says
/// nothing about whether work repeated.
fn turn_results_hash(calls: &[ToolCall], rest: &[HistoryMessage]) -> Option<u64> {
    let answers: HashMap<&str, u64> = rest
        .iter()
        .take_while(|m| m.role.as_str() == Some("tool"))
        .filter_map(|m| Some((m.tool_call_id.as_str()?, hash_content(&m.content))))
        .collect();

    // Pairs, not bare hashes: sorting results alone severs which call
    // produced which, so a two-call batch whose answers swap between
    // occurrences would compare equal.
    let mut pairs: Vec<(&str, &Value, u64)> = calls
        .iter()
        .map(|c| {
            let answer = answers.get(c.id.as_str()).copied()?;
            Some((c.name.as_str(), &c.arguments, answer))
        })
        .collect::<Option<Vec<_>>>()?;
    // `Value` is not `Ord`/`Hash`, and it is a `BTreeMap` underneath, so its
    // rendering is key-canonical and stands in for both.
    let mut keyed: Vec<(String, u64)> = pairs
        .drain(..)
        .map(|(name, args, answer)| (format!("{name}\u{0}{args}"), answer))
        .collect();
    keyed.sort_unstable();

    let mut hasher = DefaultHasher::new();
    keyed.hash(&mut hasher);
    Some(hasher.finish())
}

/// Hash a tool result's content for equality alone.
///
/// Deliberately *not* [`extract_text`]: that projects to the empty string for
/// objects, numbers, nulls and non-text parts, which would make two different
/// structured results compare equal — a manufactured "identical" repeat. The
/// value is hashed as it arrived, and the string case avoids a copy because
/// tool results run to tens of kilobytes on this pre-admission path.
fn hash_content(content: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    match content {
        // Discriminated so `null` and the string "null" cannot collide, in a
        // function whose only job is equality.
        Value::String(s) => (0u8, s).hash(&mut hasher),
        other => (1u8, other.to_string()).hash(&mut hasher),
    }
    hasher.finish()
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

/// Extract the assistant-visible text from an OpenAI `content` value.
///
/// `content` may be a plain string, `null` (tool-call-only turns), or an
/// array of typed parts; only `{"type": "text"}` parts contribute.  Anything
/// else (images, unknown part types) yields the empty string, which the
/// stagnation detector ignores.
fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Bridge the OpenAI wire tool call (arguments as a JSON string) to the
/// domain [`ToolCall`] (arguments as a [`Value`]) the detectors hash.
///
/// A malformed arguments string falls back to hashing the raw string —
/// identical malformed batches still count as repeats.
fn to_domain_call(call: &WireToolCall) -> ToolCall {
    // A JSON-encoded string is the documented shape, a bare object is the
    // common deviation, and anything else is used as it stands rather than
    // rejected — this runs on content the guard only inspects.
    let arguments = match &call.function.arguments {
        Value::String(s) => {
            serde_json::from_str::<Value>(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        other => other.clone(),
    };
    ToolCall {
        id: call.id.as_str().unwrap_or_default().to_owned(),
        name: call.function.name.as_str().unwrap_or_default().to_owned(),
        arguments,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "loop_guard_tests.rs"]
mod tests;
