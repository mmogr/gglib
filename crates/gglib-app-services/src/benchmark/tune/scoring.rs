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
//! - **Ordering respected only when requested.** If none of a task's
//!   expected calls set `ordered: true`, matching is best-effort (greedy):
//!   each expected call is paired with whichever *unused* recorded call
//!   scores highest against it. If any expected call sets `ordered: true`,
//!   matching is positional instead.
//! - **Strict irrelevance.** For [`ExpectedOutcome::NoToolCall`], any
//!   recorded tool call at all yields a score of `0.0`.

use gglib_core::domain::ToolCall;
use gglib_core::domain::benchmark::tune::task::{ExpectedCall, ExpectedOutcome};
use serde_json::Value;

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

/// Score `recorded` tool calls against `expected`.
#[must_use]
pub fn score_outcome(expected: &ExpectedOutcome, recorded: &[ToolCall]) -> ScoreOutcome {
    match expected {
        ExpectedOutcome::NoToolCall => score_no_tool_call(recorded),
        ExpectedOutcome::ToolCalls { calls } => score_tool_calls(calls, recorded),
    }
}

fn score_no_tool_call(recorded: &[ToolCall]) -> ScoreOutcome {
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

fn score_tool_calls(expected: &[ExpectedCall], recorded: &[ToolCall]) -> ScoreOutcome {
    if expected.is_empty() {
        // Nothing was required — trivially satisfied.
        return ScoreOutcome {
            tool_match_score: 1.0,
            passed: true,
            detail: None,
        };
    }

    let (total, unmatched) = if expected.iter().any(|c| c.ordered) {
        score_ordered(expected, recorded)
    } else {
        score_greedy(expected, recorded)
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

/// Positional matching: `expected[i]` is compared against `recorded[i]`.
/// Returns `(sum of per-call scores, count of imperfect matches)`.
fn score_ordered(expected: &[ExpectedCall], recorded: &[ToolCall]) -> (f64, usize) {
    let mut total = 0.0;
    let mut unmatched = 0;
    for (i, exp) in expected.iter().enumerate() {
        let score = recorded
            .get(i)
            .map_or(0.0, |call| call_match_score(exp, call));
        if score < 1.0 {
            unmatched += 1;
        }
        total += score;
    }
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
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "call_1".to_string(),
            name: name.to_string(),
            arguments: args,
        }
    }

    fn expected_call(name: &str, required_args: Value, ordered: bool) -> ExpectedCall {
        ExpectedCall {
            name: name.to_string(),
            required_args: required_args.as_object().cloned().unwrap_or_default(),
            ordered,
        }
    }

    #[test]
    fn no_tool_call_passes_when_none_recorded() {
        let outcome = score_outcome(&ExpectedOutcome::NoToolCall, &[]);
        assert_eq!(outcome.tool_match_score, 1.0);
        assert!(outcome.passed);
    }

    #[test]
    fn no_tool_call_fails_strictly_on_any_call() {
        let recorded = vec![call("get_weather", json!({"location": "Boston"}))];
        let outcome = score_outcome(&ExpectedOutcome::NoToolCall, &recorded);
        assert_eq!(outcome.tool_match_score, 0.0);
        assert!(!outcome.passed);
    }

    #[test]
    fn exact_match_scores_one() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call(
                "get_weather",
                json!({"location": "Boston"}),
                false,
            )],
        };
        let recorded = vec![call("get_weather", json!({"location": "Boston"}))];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 1.0);
        assert!(outcome.passed);
    }

    #[test]
    fn extra_arguments_are_not_penalized() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call(
                "get_weather",
                json!({"location": "Boston"}),
                false,
            )],
        };
        let recorded = vec![call(
            "get_weather",
            json!({"location": "Boston", "units": "fahrenheit"}),
        )];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 1.0);
    }

    #[test]
    fn missing_required_arg_penalizes_proportionally() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call(
                "move_file",
                json!({"from": "a.txt", "to": "b.txt"}),
                false,
            )],
        };
        // Only one of two required args present.
        let recorded = vec![call("move_file", json!({"from": "a.txt"}))];
        let outcome = score_outcome(&expected, &recorded);
        assert!((outcome.tool_match_score - 0.5).abs() < 1e-9);
        assert!(!outcome.passed);
    }

    #[test]
    fn wrong_tool_name_scores_zero_regardless_of_args() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call(
                "get_weather",
                json!({"location": "Boston"}),
                false,
            )],
        };
        let recorded = vec![call("get_weather_v2", json!({"location": "Boston"}))];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 0.0);
    }

    #[test]
    fn numeric_type_mismatch_is_not_penalized_int_vs_float() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call("set_temp", json!({"value": 72}), false)],
        };
        // Model supplies a float where expected value is an int — must match.
        let recorded = vec![call("set_temp", json!({"value": 72.0}))];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 1.0);
    }

    #[test]
    fn unordered_calls_use_greedy_best_effort_matching() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![
                expected_call("get_weather", json!({"location": "Austin"}), false),
                expected_call("get_weather", json!({"location": "Boston"}), false),
            ],
        };
        // Recorded in the opposite order — must still match both.
        let recorded = vec![
            call("get_weather", json!({"location": "Boston"})),
            call("get_weather", json!({"location": "Austin"})),
        ];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 1.0);
    }

    #[test]
    fn ordered_calls_require_positional_match() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![
                expected_call("search_files", json!({}), true),
                expected_call("read_file", json!({}), true),
            ],
        };
        // Wrong order relative to expected positions.
        let recorded = vec![
            call("read_file", json!({})),
            call("search_files", json!({})),
        ];
        let outcome = score_outcome(&expected, &recorded);
        assert_eq!(outcome.tool_match_score, 0.0);
    }

    #[test]
    fn missing_call_entirely_scores_zero_for_that_expectation() {
        let expected = ExpectedOutcome::ToolCalls {
            calls: vec![expected_call("get_weather", json!({}), false)],
        };
        let outcome = score_outcome(&expected, &[]);
        assert_eq!(outcome.tool_match_score, 0.0);
        assert!(!outcome.passed);
    }
}
