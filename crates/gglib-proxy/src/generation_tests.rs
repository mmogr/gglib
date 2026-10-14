//! Tests for [`super`]: what a non-streaming body reads as, how the readings
//! reach a request's snapshot, and the keys they serialise under.
//!
//! The drain's side of the metering is in `forward_generation_tests.rs`.

use serde_json::json;

use super::*;
use crate::metrics::{ContextMetricsStore, ContextSnapshot};

fn snapshot(model: &str) -> ContextSnapshot {
    ContextSnapshot {
        model_name: model.to_owned(),
        payload_chars_before: 1_000,
        payload_chars_after: 800,
        messages_truncated: 1,
        was_clamped: false,
        grammar_enforced: false,
        dialect_residue: false,
        tool_repaired: false,
        completion_tokens: None,
        reasoning_chars: 0,
        tool_calls: 0,
        finish_reason: None,
        generation_ms: None,
        loop_guard_trip: None,
        recorded_at_secs: 7,
        seq: 0,
    }
}

/// Readings no default equals, and no two of which are equal.
fn readings() -> TurnGeneration {
    TurnGeneration {
        completion_tokens: Some(37),
        reasoning_chars: 12,
        tool_calls: 2,
        finish_reason: Some("tool_calls".to_owned()),
        generation_ms: Some(130),
    }
}

/// Every snapshot in `store`, as the status route serialises it.
fn rows(store: &ContextMetricsStore) -> Vec<serde_json::Value> {
    store
        .recent(usize::MAX)
        .iter()
        .map(|row| serde_json::to_value(row).expect("a snapshot serialises"))
        .collect()
}

#[test]
fn a_unary_body_is_read_for_what_it_carries() {
    let call = json!({"id": "c", "type": "function", "function": {"name": "f", "arguments": "{}"}});
    let body = json!({
        "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "reasoning_content": "plan: résumé",
            "tool_calls": [call, call, call]
        }}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 37, "total_tokens": 48}
    });

    assert_eq!(
        TurnGeneration::of_unary_body(body.to_string().as_bytes()),
        TurnGeneration {
            completion_tokens: Some(37),
            reasoning_chars: 12,
            tool_calls: 3,
            finish_reason: Some("tool_calls".to_owned()),
            generation_ms: None,
        },
        "twelve characters in fourteen bytes, and no deltas to time"
    );
}

/// A body with no usage, reasoning, calls or finish reason reads as none of
/// them, and so does one that is not JSON.
#[test]
fn a_unary_body_that_carries_nothing_reads_as_nothing() {
    let bare =
        json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}}]});

    assert_eq!(
        TurnGeneration::of_unary_body(bare.to_string().as_bytes()),
        TurnGeneration::default()
    );
    assert_eq!(
        TurnGeneration::of_unary_body(b"upstream said something else"),
        TurnGeneration::default()
    );
    assert_eq!(TurnGeneration::default().completion_tokens, None);
    assert_eq!(TurnGeneration::default().generation_ms, None);
}

#[test]
fn the_readings_are_written_onto_the_snapshot_with_that_seq() {
    let store = ContextMetricsStore::new();
    let first = store.record(snapshot("a"));
    let second = store.record(snapshot("b"));
    let third = store.record(snapshot("c"));

    store.flag_generation(second, readings());

    let by_seq = |seq| {
        let rows = store.recent(usize::MAX);
        let row = rows.iter().find(|row| row.seq == seq).expect("recorded");
        (row.model_name.clone(), TurnGeneration::on(row))
    };
    assert_eq!(by_seq(second), ("b".to_owned(), readings()));
    assert_eq!(by_seq(first), ("a".to_owned(), TurnGeneration::default()));
    assert_eq!(by_seq(third), ("c".to_owned(), TurnGeneration::default()));
}

#[test]
fn a_seq_no_snapshot_has_changes_nothing() {
    let store = ContextMetricsStore::new();
    let recorded = store.record(snapshot("a"));
    store.record(snapshot("b"));
    let before = rows(&store);

    store.flag_generation(recorded + 40, readings());

    assert_eq!(rows(&store), before);
    assert_eq!(store.total_requests(), 2);
}

/// The readings sit beside the snapshot's other keys, not under one of
/// their own, and the other keys are as they were.
#[test]
fn the_readings_serialise_beside_the_snapshots_other_keys() {
    let store = ContextMetricsStore::new();
    let seq = store.record(snapshot("a"));
    let mut expected = rows(&store).remove(0);
    assert_eq!(
        expected,
        json!({
            "model_name": "a",
            "payload_chars_before": 1_000,
            "payload_chars_after": 800,
            "messages_truncated": 1,
            "was_clamped": false,
            "grammar_enforced": false,
            "dialect_residue": false,
            "tool_repaired": false,
            "loop_guard_trip": null,
            "recorded_at_secs": 7,
            "completion_tokens": null,
            "reasoning_chars": 0,
            "tool_calls": 0,
            "finish_reason": null,
            "generation_ms": null,
        }),
        "a turn not yet over, or never forwarded"
    );

    store.flag_generation(seq, readings());

    expected["completion_tokens"] = json!(37);
    expected["reasoning_chars"] = json!(12);
    expected["tool_calls"] = json!(2);
    expected["finish_reason"] = json!("tool_calls");
    expected["generation_ms"] = json!(130);
    assert_eq!(rows(&store).remove(0), expected);
}
