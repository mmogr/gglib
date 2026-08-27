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

/// Key canonicalisation is `stable_repr`'s own, not `serde_json::Value`'s.
///
/// This used to ride on `Value` being a `BTreeMap`, so enabling that crate's
/// `preserve_order` feature would have made the rendering insertion-ordered and
/// this join would have started silently under-reporting. `stable_repr` sorts
/// keys itself, so the dependence is gone — and the constraint should not be
/// carried forward by anyone reading this test.
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

/// Arguments the signature cannot tell apart must not be told apart here.
///
/// `stable_repr` collapses everything below `MAX_REPR_DEPTH` to a sentinel, so
/// two batches nested deeper than that share one signature and therefore one
/// run. Rendering the arguments in full here would give them a different
/// answers hash on every occurrence — a rescue that never ends, and for an
/// observation-tier batch a guard that can never fire. The depth cap's own
/// safety argument is that a collision can only make the guard stricter; this
/// is what keeps that true.
#[test]
fn arguments_deeper_than_the_signature_can_see_hash_alike_here_too() {
    fn nest(depth: usize) -> serde_json::Value {
        let mut v = json!("leaf");
        for _ in 0..depth {
            v = json!({ "x": v });
        }
        v
    }
    let (a, b) = (
        vec![call("c1", "probe", nest(20))],
        vec![call("c1", "probe", nest(21))],
    );
    // Same signature: the detector treats these as one run.
    assert_eq!(
        crate::domain::agent::batch_signature(&a),
        crate::domain::agent::batch_signature(&b)
    );
    let answer = vec![Some(hash_result_text("same"))];
    assert_eq!(
        batch_results_hash(&a, &answer),
        batch_results_hash(&b, &answer),
        "one run must not see a changing answer purely from argument depth"
    );
}

/// The NUL between a call's name and its arguments is load bearing.
///
/// Without it the key is a bare concatenation, so `t` + `123` and `t1` + `23`
/// are the same string. The proxy reaches this: its wire types accept a
/// non-object `arguments`, so `"123"` parses to a bare number. `batch_signature`
/// still calls such a batch one run, so if the two calls' answers swap between
/// occurrences the join reports "answer unchanged" and takes a strike the model
/// did not earn — a false rejection, which is the failure class this whole
/// change exists to remove.
///
/// The existing swap test cannot reach it: object arguments render with braces,
/// and no suffix collision is constructible through them.
#[test]
fn the_separator_stops_a_name_and_argument_suffix_collision() {
    let (a, b) = (
        vec![call("c1", "t", json!(123)), call("c2", "t1", json!(23))],
        vec![call("c1", "t", json!(123)), call("c2", "t1", json!(23))],
    );
    let (x, y) = (hash_result_text("alpha"), hash_result_text("beta"));
    // Same calls, answers swapped between the two occurrences. A collision in
    // the pair keys would make these compare equal.
    assert_ne!(
        batch_results_hash(&a, &[Some(x), Some(y)]),
        batch_results_hash(&b, &[Some(y), Some(x)]),
        "the pair keys must stay distinct, or swapped answers read as unchanged"
    );
}

/// The discriminant on the text path, for the same reason as the other arm.
///
/// Without the leading `0u8` a result whose text happens to equal the JSON
/// rendering of a non-string value hashes alike. Not reachable from a real
/// tool, but the arm's whole job is that two shapes cannot collide, and the
/// other arm is tested.
#[test]
fn the_text_discriminant_keeps_shapes_apart() {
    assert_ne!(
        hash_result_text("\u{1}42"),
        hash_result_content(&json!(42)),
        "a crafted text must not collide with a number's rendering"
    );
}
