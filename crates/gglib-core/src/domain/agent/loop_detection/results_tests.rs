//! Tests for the results join.
//!
//! Split into its own file for the same reason `loop_detection/tests.rs` is:
//! the file-size ratchet counts test code, and a module's tests should not be
//! what pushes its implementation over the budget.

use serde_json::json;

use super::*;

fn call(id: &str, name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: args,
    }
}

// =============================================================================
// One answer
// =============================================================================

/// The zero-copy text path and the general path must agree, or the agent loop
/// and the proxy would compute different hashes for the same string answer —
/// the ordinary case, and the one where parity is easiest to lose silently.
#[test]
fn text_and_string_value_answers_hash_alike() {
    assert_eq!(
        hash_result_text("1 file changed"),
        hash_result_content(&json!("1 file changed"))
    );
}

/// A function whose only job is equality must not let a null and the string
/// spelling of one collide.
#[test]
fn null_and_the_string_null_do_not_collide() {
    assert_ne!(
        hash_result_content(&json!(null)),
        hash_result_content(&json!("null"))
    );
}

/// The reason this is not a text projection: projecting objects to `""` would
/// report two different structured answers as an identical repeat.
#[test]
fn structured_answers_that_differ_hash_differently() {
    assert_ne!(
        hash_result_content(&json!({"files": 1})),
        hash_result_content(&json!({"files": 2}))
    );
}

// =============================================================================
// One batch
// =============================================================================

#[test]
fn the_same_batch_and_answers_hash_the_same() {
    let calls = vec![call("c1", "read_file", json!({"path": "a.rs"}))];
    let answers = vec![Some(hash_result_text("contents"))];
    assert_eq!(
        batch_results_hash(&calls, &answers),
        batch_results_hash(&calls, &answers)
    );
}

/// The property the pairing exists for. Both occurrences have the same two
/// calls and the same two answers; only which call got which changed. Sorting
/// bare answer hashes would call these equal.
#[test]
fn a_two_call_batch_whose_answers_swap_is_not_identical() {
    let calls = vec![
        call("c1", "read_file", json!({"path": "a.rs"})),
        call("c2", "read_file", json!({"path": "b.rs"})),
    ];
    let (a, b) = (hash_result_text("alpha"), hash_result_text("beta"));
    assert_ne!(
        batch_results_hash(&calls, &[Some(a), Some(b)]),
        batch_results_hash(&calls, &[Some(b), Some(a)])
    );
}

/// `batch_signature` sorts, so the same parallel batch re-emitted in a
/// different order is one batch. This join has to agree, or a model that
/// shuffles its calls would look like it kept getting new answers.
#[test]
fn the_same_pairs_in_a_different_order_hash_alike() {
    let a = call("c1", "read_file", json!({"path": "a.rs"}));
    let b = call("c2", "read_file", json!({"path": "b.rs"}));
    let (ha, hb) = (hash_result_text("alpha"), hash_result_text("beta"));
    assert_eq!(
        batch_results_hash(&[a.clone(), b.clone()], &[Some(ha), Some(hb)]),
        batch_results_hash(&[b, a], &[Some(hb), Some(ha)])
    );
}

/// A partially-answered batch says nothing about whether work repeated, so it
/// must not be reported as anything.
#[test]
fn an_unanswered_call_makes_the_whole_batch_unjoinable() {
    let calls = vec![
        call("c1", "read_file", json!({"path": "a.rs"})),
        call("c2", "read_file", json!({"path": "b.rs"})),
    ];
    assert!(batch_results_hash(&calls, &[Some(1), None]).is_none());
}

/// A caller that has lost track of which answer belongs to which call has not
/// produced a weaker reading; it has produced no reading at all.
#[test]
fn a_length_mismatch_is_unjoinable_rather_than_truncated() {
    let calls = vec![
        call("c1", "read_file", json!({"path": "a.rs"})),
        call("c2", "read_file", json!({"path": "b.rs"})),
    ];
    assert!(batch_results_hash(&calls, &[Some(1)]).is_none());
    assert!(batch_results_hash(&calls, &[Some(1), Some(2), Some(3)]).is_none());
}

/// Different arguments are a different pair key even when the answers match,
/// so polling two files that happen to read alike is not one repeated call.
#[test]
fn the_same_answer_to_a_different_call_is_a_different_batch() {
    let (a, b) = (
        vec![call("c1", "read_file", json!({"path": "a.rs"}))],
        vec![call("c1", "read_file", json!({"path": "b.rs"}))],
    );
    let answer = vec![Some(hash_result_text("same"))];
    assert_ne!(
        batch_results_hash(&a, &answer),
        batch_results_hash(&b, &answer)
    );
}

/// Key canonicalisation rides on `serde_json::Value` being a `BTreeMap`. If
/// `preserve_order` were ever enabled, the rendering would become
/// insertion-ordered and this join would silently start under-reporting.
#[test]
fn shuffled_argument_keys_hash_alike() {
    let (a, b) = (
        vec![call("c1", "write", json!({"a": 1, "b": 2}))],
        vec![call("c1", "write", json!({"b": 2, "a": 1}))],
    );
    let answer = vec![Some(hash_result_text("ok"))];
    assert_eq!(
        batch_results_hash(&a, &answer),
        batch_results_hash(&b, &answer)
    );
}
