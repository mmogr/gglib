//! Tests for batch signatures.
//!
//! Moved here with the code they cover. They were in the detector's test file
//! while the two lived in one module, and the detector's own tests are about a
//! different question entirely.

use serde_json::json;

use super::*;
use crate::domain::agent::tool_types::ToolCall;

// ---- tool_signature / batch_signature ---------------------------------------
// Note: fnv1a_64 correctness is covered in crates/gglib-core/src/domain/agent/fnv1a.rs.

#[test]
fn tool_signature_includes_name_and_hash() {
    let call = ToolCall {
        id: "c1".into(),
        name: "fs_read".into(),
        arguments: json!({ "path": "/etc/hosts" }),
    };
    let sig = tool_signature(&call);
    assert!(sig.starts_with("fs_read:"), "should start with name: {sig}");
    assert_eq!(
        sig.len(),
        "fs_read:".len() + 16,
        "hash should be 16 hex chars"
    );
}

#[test]
fn batch_signature_is_sorted() {
    // Regardless of input order, the batch signature must be consistent.
    let calls = vec![
        ToolCall {
            id: "c1".into(),
            name: "b_tool".into(),
            arguments: json!({}),
        },
        ToolCall {
            id: "c2".into(),
            name: "a_tool".into(),
            arguments: json!({}),
        },
    ];
    let sig = batch_signature(&calls);
    let parts: Vec<&str> = sig.split('|').collect();
    assert_eq!(parts.len(), 2);
    let mut sorted = parts.clone();
    sorted.sort_unstable();
    assert_eq!(
        parts, sorted,
        "batch signature parts should be lexicographically sorted"
    );
}

#[test]
fn batch_signature_ignores_call_order() {
    let a = ToolCall {
        id: "c1".into(),
        name: "search".into(),
        arguments: json!({ "q": "rust" }),
    };
    let b = ToolCall {
        id: "c2".into(),
        name: "read".into(),
        arguments: json!({ "path": "/" }),
    };
    assert_eq!(
        batch_signature(&[a.clone(), b.clone()]),
        batch_signature(&[b, a])
    );
}

#[test]
fn different_argument_key_order_produces_same_signature() {
    // Without canonical_json, serde_json preserves insertion order so
    // {"a":1,"b":2} and {"b":2,"a":1} produce different to_string output.
    // canonical_json must normalise them to the same string.
    let call_ab = ToolCall {
        id: "c1".into(),
        name: "tool".into(),
        arguments: json!({ "a": 1, "b": 2 }),
    };
    let call_ba = ToolCall {
        id: "c1".into(),
        name: "tool".into(),
        arguments: serde_json::from_str::<serde_json::Value>(r#"{"b":2,"a":1}"#).unwrap(),
    };
    assert_eq!(
        tool_signature(&call_ab),
        tool_signature(&call_ba),
        "signatures must match regardless of JSON key ordering"
    );
}

#[test]
fn nested_argument_objects_are_canonicalised() {
    let call_sorted = ToolCall {
        id: "c1".into(),
        name: "t".into(),
        arguments: json!({ "outer": { "x": 1, "y": 2 } }),
    };
    let call_unsorted = ToolCall {
        id: "c1".into(),
        name: "t".into(),
        arguments: serde_json::from_str::<serde_json::Value>(r#"{"outer":{"y":2,"x":1}}"#).unwrap(),
    };
    assert_eq!(
        tool_signature(&call_sorted),
        tool_signature(&call_unsorted),
        "nested object keys must also be sorted"
    );
}

// ---- stable_repr / MAX_REPR_DEPTH guard -------------------------------------

/// Build a JSON object nested `depth` levels deep: `{"x": {"x": ... }}`.
fn nested_object(depth: usize) -> serde_json::Value {
    let mut v = json!("leaf");
    for _ in 0..depth {
        v = json!({ "x": v });
    }
    v
}

#[test]
fn stable_repr_caps_recursion_at_max_depth() {
    // Build an object that is one level deeper than the guard threshold.
    // At depth == MAX_REPR_DEPTH the inner call should return "\"...\"".
    // The important assertion is that the function returns at all (no
    // stack overflow) and that the sentinel appears somewhere in the output.
    let deep = nested_object(MAX_REPR_DEPTH + 1);
    let repr = stable_repr(&deep);
    assert!(
        repr.contains("\"...\""),
        "stable_repr of a {}-level nested object must contain sentinel; got: {repr}",
        MAX_REPR_DEPTH + 1,
    );
}

#[test]
fn stable_repr_at_exact_max_depth_triggers_sentinel() {
    // An object at exactly MAX_REPR_DEPTH levels must also trigger the cap,
    // since depth >= MAX_REPR_DEPTH is the guard condition.
    let at_limit = nested_object(MAX_REPR_DEPTH);
    let repr = stable_repr(&at_limit);
    assert!(
        repr.contains("\"...\""),
        "stable_repr at depth=MAX_REPR_DEPTH must hit the sentinel; got: {repr}",
    );
}

#[test]
fn stable_repr_below_max_depth_produces_full_output() {
    // An object strictly shallower than the limit must not be truncated.
    let shallow = nested_object(MAX_REPR_DEPTH - 1);
    let repr = stable_repr(&shallow);
    assert!(
        !repr.contains("\"...\""),
        "stable_repr at depth=MAX_REPR_DEPTH-1 must not sentinel; got: {repr}",
    );
}
