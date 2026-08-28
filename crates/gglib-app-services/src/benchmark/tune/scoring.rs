//! AST-style scoring of recorded tool calls against a task's expected outcome.
//!
//! Follows the Berkeley Function Calling Leaderboard (BFCL) approach:
//! structural comparison of tool name + arguments, not a string diff.
//!
//! # Scoring rules
//!
//! - **Extra arguments are ignored.** No penalty for hallucinated optional
//!   keys as long as every required key is correct.
//! - **Missing arguments penalize.** A call's score is
//!   `matching_required_args / total_required_args`.
//! - **Type-safe value matching.** Values are compared structurally (a JSON
//!   `1` matches a JSON `1.0`), never via string diffing.
//! - **Ordering respected only when requested, and only across batches.** If
//!   none of a task's expected calls set `ordered: true`, matching is
//!   best-effort (greedy): each expected call is paired with whichever
//!   *unused* recorded call scores highest against it. If any expected call
//!   sets `ordered: true`, expected calls are consumed in order **batch by
//!   batch**, freely within each batch.
//! - **Result dependencies need a later batch.** A call marked
//!   `depends_on_result` cannot be credited from the same batch as the call it
//!   follows. See [`ExpectedCall::depends_on_result`].
//! - **Strict irrelevance.** For [`ExpectedOutcome::NoToolCall`], any
//!   recorded tool call at all yields a score of `0.0`.
//!
//! # Why batches rather than a flat list
//!
//! The agent loop executes a parallel batch by spawning every call into a
//! `JoinSet`, and the scoring executor appends to its log from inside each
//! task — so the flat log is in **completion** order, which is a scheduler
//! coin-flip, not the order the model emitted. Positional matching over that
//! log scored a correct single-batch answer `0.0` whenever the two tasks
//! happened to finish the other way round, and it did so asymmetrically: only
//! an arm that batches its calls could ever lose the toss.
//!
//! Batch boundaries are the fix and the only real signal available. Within a
//! batch there is no order to check; across batches there is, and it is the
//! model's own.

use gglib_core::domain::ToolCall;
use gglib_core::domain::benchmark::tune::task::{ExpectedCall, ExpectedOutcome};
use serde_json::Value;

/// Tool calls grouped by the agent-loop iteration that executed them.
pub type CallBatches = [Vec<ToolCall>];

/// Result of scoring one task's recorded tool calls against its expectation.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreOutcome {
    /// AST-style match score, `0.0`–`1.0`.
    pub tool_match_score: f64,
    /// `true` only for an exact match (`tool_match_score == 1.0`).
    pub passed: bool,
    /// Human-readable explanation when `passed` is `false`.
    pub detail: Option<String>,
}

/// Score `batches` of recorded tool calls against `expected`.
///
/// `batches` is one entry per agent-loop iteration that executed tool calls,
/// in iteration order. A single-turn task has one batch; a task the model
/// answered in two turns has two.
#[must_use]
pub fn score_outcome(expected: &ExpectedOutcome, batches: &CallBatches) -> ScoreOutcome {
    match expected {
        ExpectedOutcome::NoToolCall => score_no_tool_call(batches),
        ExpectedOutcome::ToolCalls { calls } => score_tool_calls(calls, batches),
    }
}

/// Every recorded call, batch boundaries discarded. For the paths where
/// ordering plays no part and only the multiset matters.
fn flatten(batches: &CallBatches) -> Vec<ToolCall> {
    batches.iter().flatten().cloned().collect()
}

fn score_no_tool_call(batches: &CallBatches) -> ScoreOutcome {
    let recorded = flatten(batches);
    if recorded.is_empty() {
        ScoreOutcome {
            tool_match_score: 1.0,
            passed: true,
            detail: None,
        }
    } else {
        ScoreOutcome {
            tool_match_score: 0.0,
            passed: false,
            detail: Some(format!(
                "expected no tool call but {} were made",
                recorded.len()
            )),
        }
    }
}

fn score_tool_calls(expected: &[ExpectedCall], batches: &CallBatches) -> ScoreOutcome {
    if expected.is_empty() {
        // Nothing was required — trivially satisfied.
        return ScoreOutcome {
            tool_match_score: 1.0,
            passed: true,
            detail: None,
        };
    }

    let sequenced = expected.iter().any(|c| c.ordered || c.depends_on_result);
    let (total, unmatched) = if sequenced {
        score_sequenced(expected, batches)
    } else {
        score_greedy(expected, &flatten(batches))
    };

    #[allow(clippy::cast_precision_loss)]
    let score = total / expected.len() as f64;
    let passed = (score - 1.0).abs() < 1e-9;
    let detail = (!passed).then(|| {
        format!(
            "{unmatched} of {} expected call(s) not fully matched",
            expected.len()
        )
    });

    ScoreOutcome {
        tool_match_score: score,
        passed,
        detail,
    }
}

/// Sequenced matching: expected calls are consumed in order, batch by batch,
/// with no ordering demanded *within* a batch.
///
/// Each batch takes the next run of expected calls, and matches them against
/// its own recorded calls greedily — which is correct precisely because the
/// model emitted them simultaneously and expressed no order between them. The
/// run stops early at a [`ExpectedCall::depends_on_result`] call, which cannot
/// be satisfied by the batch that carries the call it depends on.
///
/// Returns `(sum of per-call scores, count of imperfect matches)`.
fn score_sequenced(expected: &[ExpectedCall], batches: &CallBatches) -> (f64, usize) {
    let mut total = 0.0;
    let mut unmatched = 0;
    let mut next = 0;

    for batch in batches {
        if next >= expected.len() {
            break;
        }
        // How many of the remaining expectations this batch is allowed to
        // satisfy: as many as it has calls, stopping before any call that
        // needs a result this batch has not produced yet.
        let mut take = 0;
        while next + take < expected.len() && take < batch.len() {
            if take > 0 && expected[next + take].depends_on_result {
                break;
            }
            take += 1;
        }
        if take == 0 {
            continue;
        }

        let (batch_total, batch_unmatched) = score_greedy(&expected[next..next + take], batch);
        total += batch_total;
        unmatched += batch_unmatched;
        next += take;
    }

    // Expectations no batch ever reached score nothing, and say so.
    unmatched += expected.len() - next;
    (total, unmatched)
}

/// Best-effort greedy matching: each expected call (in the order given) is
/// paired with whichever *unused* recorded call scores highest against it.
/// This is a greedy approximation, not an optimal assignment — acceptable
/// because expected-call lists are small (a handful of calls per task) and
/// the greedy result differs from optimal only in rare adversarial cases.
/// Returns `(sum of per-call scores, count of imperfect matches)`.
fn score_greedy(expected: &[ExpectedCall], recorded: &[ToolCall]) -> (f64, usize) {
    let mut used = vec![false; recorded.len()];
    let mut total = 0.0;
    let mut unmatched = 0;

    for exp in expected {
        let best = recorded
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(i, call)| (i, call_match_score(exp, call)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let score = match best {
            Some((i, score)) if score > 0.0 => {
                used[i] = true;
                score
            }
            _ => 0.0,
        };
        if score < 1.0 {
            unmatched += 1;
        }
        total += score;
    }

    (total, unmatched)
}

/// Score a single recorded call against a single expected call.
///
/// Returns `0.0` immediately on a tool-name mismatch — a wrong tool can
/// never partially satisfy an expectation regardless of its arguments.
fn call_match_score(expected: &ExpectedCall, actual: &ToolCall) -> f64 {
    if expected.name != actual.name {
        return 0.0;
    }
    if expected.required_args.is_empty() {
        return 1.0;
    }

    let actual_obj = actual.arguments.as_object();
    #[allow(clippy::cast_precision_loss)]
    let total = expected.required_args.len() as f64;
    let matching = expected
        .required_args
        .iter()
        .filter(|(key, expected_value)| {
            actual_obj
                .and_then(|obj| obj.get(key.as_str()))
                .is_some_and(|actual_value| json_values_match(expected_value, actual_value))
        })
        .count();

    #[allow(clippy::cast_precision_loss)]
    let matching = matching as f64;
    matching / total
}

/// Structural JSON value equality that treats numerically-equal
/// floats/integers as a match (`1` == `1.0`), recursing into arrays/objects.
fn json_values_match(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-9,
            _ => a == b,
        },
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| json_values_match(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|bv| json_values_match(v, bv)))
        }
        _ => expected == actual,
    }
}

#[cfg(test)]
#[path = "scoring_tests.rs"]
mod scoring_tests;
