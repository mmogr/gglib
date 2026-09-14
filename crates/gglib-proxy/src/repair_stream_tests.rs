//! Tests for how a streamed turn's tool calls are reassembled for judging, and
//! how a repaired response becomes stream events again. A child of
//! `repair_tests.rs`, for its fixtures.

use super::*;

// ── Accumulator and synthesis ────────────────────────────────────────

/// Streamed deltas carry `id`/`name` only on the first fragment and split
/// `arguments` arbitrarily, so reassembly is the only way to know what was
/// emitted.
#[test]
fn fragmented_deltas_reassemble_into_one_call() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(0, Some("call_1"), Some("read_file"), Some(r#"{"path":"#));
    acc.push(0, None, None, Some(r#""a","max_lines":"#));
    acc.push(0, None, None, Some("42}"));

    let calls = acc.to_tool_calls();
    assert_eq!(calls[0]["function"]["name"], "read_file");
    assert_eq!(calls[0]["id"], "call_1");
    assert_eq!(
        calls[0]["function"]["arguments"],
        r#"{"path":"a","max_lines":42}"#
    );
}

/// Parallel calls arrive interleaved by index, not in sequence.
#[test]
fn interleaved_indices_stay_separate() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(0, Some("a"), Some("read_file"), Some(r#"{"path":"#));
    acc.push(1, Some("b"), Some("read_file"), Some(r#"{"path":"#));
    acc.push(1, None, None, Some(r#""two"}"#));
    acc.push(0, None, None, Some(r#""one"}"#));

    let calls = acc.to_tool_calls();
    assert_eq!(calls.as_array().unwrap().len(), 2);
    assert_eq!(calls[0]["function"]["arguments"], r#"{"path":"one"}"#);
    assert_eq!(calls[1]["function"]["arguments"], r#"{"path":"two"}"#);
}

/// An index arriving before its predecessors must not panic or misplace.
#[test]
fn an_out_of_order_first_index_does_not_panic() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(2, Some("c"), Some("read_file"), Some("{}"));

    let calls = acc.to_tool_calls();
    assert_eq!(calls.as_array().unwrap().len(), 3);
    assert_eq!(calls[2]["id"], "c");
}

#[test]
fn an_empty_accumulator_is_empty() {
    assert!(ToolCallAccumulator::default().is_empty());
}

/// The assembled shape must be exactly what the validator reads, or the
/// hold-back would validate something the client never receives.
#[test]
fn the_assembled_shape_validates_like_a_real_response() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(
        0,
        Some("x"),
        Some("read_file"),
        Some(r#"{"path":"a","max_lines":"42"}"#),
    );

    let wrapped = json!({"choices": [{"message": {"tool_calls": acc.to_tool_calls()}}]});
    let bytes = serde_json::to_vec(&wrapped).unwrap();

    assert!(matches!(
        decide(&request(json!("auto")), &bytes, true),
        Decision::Reissue { .. }
    ));
}

/// One event per call, not per fragment: a complete delta cannot be
/// truncated mid-arguments or interleaved wrongly on the wire.
#[test]
fn synthesis_emits_one_complete_event_per_call() {
    let body = response(r#"{"path":"a","max_lines":42}"#);
    let events = synthesize_tool_call_events(&body);

    assert_eq!(events.len(), 1);
    let LlmStreamEvent::ToolCallDelta {
        index,
        name,
        arguments,
        ..
    } = &events[0]
    else {
        panic!("expected a ToolCallDelta");
    };
    assert_eq!(*index, 0);
    assert_eq!(name.as_deref(), Some("read_file"));
    assert_eq!(arguments.as_deref(), Some(r#"{"path":"a","max_lines":42}"#));
}

#[test]
fn synthesis_of_an_unreadable_body_yields_nothing() {
    assert!(synthesize_tool_call_events(b"garbage").is_empty());
}
