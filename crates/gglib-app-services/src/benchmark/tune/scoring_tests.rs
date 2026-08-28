//! Tests for [`super::scoring`].
//!
//! The batch-shape helpers matter as much as the assertions: `one_batch`
//! and `separate_batches` are the difference between a model that answered
//! in one turn and one that waited for a result, and that difference is
//! what these rules exist to tell apart.

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
        depends_on_result: false,
    }
}

/// An expected call whose arguments come from the previous call's result.
fn dependent_call(name: &str) -> ExpectedCall {
    ExpectedCall {
        name: name.to_string(),
        required_args: serde_json::Map::new(),
        ordered: true,
        depends_on_result: true,
    }
}

/// One turn that executed every one of `calls` in a single parallel batch.
fn one_batch(calls: Vec<ToolCall>) -> Vec<Vec<ToolCall>> {
    vec![calls]
}

/// One turn per call — the shape a model that waits for each result makes.
fn separate_batches(calls: Vec<ToolCall>) -> Vec<Vec<ToolCall>> {
    calls.into_iter().map(|c| vec![c]).collect()
}

#[test]
fn no_tool_call_passes_when_none_recorded() {
    let outcome = score_outcome(&ExpectedOutcome::NoToolCall, &[]);
    assert_eq!(outcome.tool_match_score, 1.0);
    assert!(outcome.passed);
}

#[test]
fn no_tool_call_fails_strictly_on_any_call() {
    let recorded = one_batch(vec![call("get_weather", json!({"location": "Boston"}))]);
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
    let recorded = one_batch(vec![call("get_weather", json!({"location": "Boston"}))]);
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
    let recorded = one_batch(vec![call(
        "get_weather",
        json!({"location": "Boston", "units": "fahrenheit"}),
    )]);
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
    let recorded = one_batch(vec![call("move_file", json!({"from": "a.txt"}))]);
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
    let recorded = one_batch(vec![call("get_weather_v2", json!({"location": "Boston"}))]);
    let outcome = score_outcome(&expected, &recorded);
    assert_eq!(outcome.tool_match_score, 0.0);
}

#[test]
fn numeric_type_mismatch_is_not_penalized_int_vs_float() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![expected_call("set_temp", json!({"value": 72}), false)],
    };
    // Model supplies a float where expected value is an int — must match.
    let recorded = one_batch(vec![call("set_temp", json!({"value": 72.0}))]);
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
    let recorded = one_batch(vec![
        call("get_weather", json!({"location": "Boston"})),
        call("get_weather", json!({"location": "Austin"})),
    ]);
    let outcome = score_outcome(&expected, &recorded);
    assert_eq!(outcome.tool_match_score, 1.0);
}

/// **The scoring race this module was rewritten around.**
///
/// The call log is appended from inside each spawned tool task, so a
/// parallel batch lands in completion order — a tokio scheduling detail.
/// Positional matching over that log scored a correct answer `0.0` on the
/// toss, and only ever against an arm that batches its calls, which made
/// the bug look like a finding about the pipeline.
#[test]
fn an_ordered_pair_in_one_batch_scores_the_same_either_way_round() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("search_files", json!({}), true),
            expected_call("read_file", json!({}), true),
        ],
    };

    let as_emitted = one_batch(vec![
        call("search_files", json!({})),
        call("read_file", json!({})),
    ]);
    let as_completed = one_batch(vec![
        call("read_file", json!({})),
        call("search_files", json!({})),
    ]);

    assert_eq!(score_outcome(&expected, &as_emitted).tool_match_score, 1.0);
    assert_eq!(
        score_outcome(&expected, &as_completed).tool_match_score,
        1.0,
        "within one batch the model expressed no order, so neither log may lose"
    );
}

/// Ordering across batches is the model's own and is still enforced.
#[test]
fn ordered_calls_in_the_wrong_batches_still_fail() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("search_files", json!({}), true),
            expected_call("read_file", json!({}), true),
        ],
    };
    let recorded = separate_batches(vec![
        call("read_file", json!({})),
        call("search_files", json!({})),
    ]);
    let outcome = score_outcome(&expected, &recorded);
    assert_eq!(outcome.tool_match_score, 0.0);
}

/// A call that needs the previous call's result cannot be credited from
/// the batch that produced it: the model deleted the file without ever
/// seeing whether it was there.
#[test]
fn a_dependent_call_gets_no_credit_from_its_own_batch() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("file_exists", json!({}), true),
            dependent_call("delete_file"),
        ],
    };
    let recorded = one_batch(vec![
        call("file_exists", json!({})),
        call("delete_file", json!({})),
    ]);

    let outcome = score_outcome(&expected, &recorded);
    assert!(
        (outcome.tool_match_score - 0.5).abs() < 1e-9,
        "the check earns its half; the blind delete earns nothing"
    );
    assert!(!outcome.passed);
}

#[test]
fn a_dependent_call_passes_when_it_waits_for_the_result() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("file_exists", json!({}), true),
            dependent_call("delete_file"),
        ],
    };
    let recorded = separate_batches(vec![
        call("file_exists", json!({})),
        call("delete_file", json!({})),
    ]);

    let outcome = score_outcome(&expected, &recorded);
    assert_eq!(outcome.tool_match_score, 1.0);
    assert!(outcome.passed);
}

/// The deliberate non-application of `depends_on_result`. Appending to a
/// path you already know needs no intervening result, so a model that does
/// both at once is more efficient rather than skipping a step.
#[test]
fn an_independent_ordered_pair_may_be_answered_in_one_batch() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("create_file", json!({"path": "a.txt"}), true),
            expected_call("append_file", json!({"path": "a.txt"}), true),
        ],
    };
    let recorded = one_batch(vec![
        call("create_file", json!({"path": "a.txt"})),
        call("append_file", json!({"path": "a.txt"})),
    ]);

    let outcome = score_outcome(&expected, &recorded);
    assert_eq!(outcome.tool_match_score, 1.0);
    assert!(outcome.passed);
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

/// A second batch cannot re-earn credit an earlier one already spent.
#[test]
fn a_repeated_batch_does_not_double_count() {
    let expected = ExpectedOutcome::ToolCalls {
        calls: vec![
            expected_call("file_exists", json!({}), true),
            dependent_call("delete_file"),
        ],
    };
    // The model checked, then checked again, and never deleted.
    let recorded = separate_batches(vec![
        call("file_exists", json!({})),
        call("file_exists", json!({})),
    ]);

    let outcome = score_outcome(&expected, &recorded);
    assert!((outcome.tool_match_score - 0.5).abs() < 1e-9);
    assert!(!outcome.passed);
}
