//! Tool-call loop detection via FNV-1a batch signatures.
//!
//! # Algorithm
//!
//! 1. Compute an **individual signature** for each [`ToolCall`] as
//!    `"{name}:{fnv1a_64(canonical_args_json):016x}"`.
//! 2. Sort the individual signatures and join them with `"|"` to form a
//!    **batch signature** that is independent of tool-call ordering.
//! 3. A [`LoopDetector`] counts how many times each batch signature has been
//!    seen.  When the count exceeds `max_repeated_batch_steps` the loop is
//!    considered stuck and [`AgentError::LoopDetected`] is returned.
//!
//! ## Hash algorithm
//!
//! FNV-1a 64-bit with:
//! - Offset basis: `14_695_981_039_346_656_037`
//! - Prime: `1_099_511_628_211`
//! - Wrapping 64-bit multiplication (`wrapping_mul`)
//!
//! Argument JSON objects are **canonicalised** (keys sorted recursively)
//! before hashing so that `{"a":1,"b":2}` and `{"b":2,"a":1}` produce the
//! same signature, preventing a non-deterministically ordered model from
//! bypassing the loop guard.

use std::collections::HashMap;

use gglib_core::ToolCall;
use gglib_core::ports::AgentError;
use serde_json::Value;

use crate::fnv1a::fnv1a_64;

// =============================================================================
// Signature helpers
// =============================================================================

/// Serialise a [`serde_json::Value`] to a canonical JSON string with object
/// keys sorted recursively so that `{"b":2,"a":1}` and `{"a":1,"b":2}`
/// produce identical output.  Array element order is preserved.
fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
            pairs.sort_unstable_by_key(|(k, _)| k.as_str());
            let inner = pairs
                .into_iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).expect("in-memory String serialisation is infallible"),
                        canonical_json(v)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Value::Array(arr) => {
            let inner = arr.iter().map(canonical_json).collect::<Vec<_>>().join(",");
            format!("[{inner}]")
        }
        _ => v.to_string(),
    }
}

/// Compute the individual signature for a single [`ToolCall`].
///
/// Format: `"{name}:{fnv1a_64(canonical_args_json):016x}"`
///
/// Arguments are serialised via [`canonical_json`] before hashing so that
/// logically identical arguments always hash identically regardless of JSON
/// key ordering.
fn tool_signature(call: &ToolCall) -> String {
    let canonical = canonical_json(&call.arguments);
    format!("{}:{:016x}", call.name, fnv1a_64(&canonical))
}

/// Compute the batch signature for a slice of [`ToolCall`]s.
///
/// Individual signatures are sorted before joining so that the result is
/// independent of the order in which the LLM emitted the calls.
fn batch_signature(calls: &[ToolCall]) -> String {
    let mut sigs: Vec<String> = calls.iter().map(tool_signature).collect();
    sigs.sort_unstable();
    sigs.join("|")
}

// =============================================================================
// LoopDetector
// =============================================================================

/// Stateful guard that detects when the same tool-call batch repeats.
///
/// Create once per agent run and call [`LoopDetector::check`] after every
/// iteration that produces tool calls.
#[derive(Debug, Default)]
pub(crate) struct LoopDetector {
    hits: HashMap<String, usize>,
}

impl LoopDetector {
    /// Record the current batch of tool calls and error if a loop is detected.
    ///
    /// A loop is declared when the same batch signature has been seen more
    /// than `max_strikes` times.  The count is incremented **before** the
    /// comparison so that `max_strikes = 2` allows two identical batches
    /// before erroring on the third, matching the frontend's
    /// `MAX_SAME_SIGNATURE_HITS = 2` behaviour.
    pub(crate) fn check(&mut self, calls: &[ToolCall], max_strikes: usize) -> Result<(), AgentError> {
        let sig = batch_signature(calls);
        let count = self.hits.entry(sig.clone()).or_insert(0);
        *count += 1;
        let count = *count;
        if count > max_strikes {
            return Err(AgentError::LoopDetected { signature: sig });
        }
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // ---- tool_signature / batch_signature -----------------------------------
    // Note: fnv1a_64 correctness is covered in crates/gglib-agent/src/fnv1a.rs.

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
            arguments: serde_json::from_str::<Value>(r#"{"b":2,"a":1}"#).unwrap(),
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
            arguments: serde_json::from_str::<Value>(r#"{"outer":{"y":2,"x":1}}"#).unwrap(),
        };
        assert_eq!(
            tool_signature(&call_sorted),
            tool_signature(&call_unsorted),
            "nested object keys must also be sorted"
        );
    }

    // ---- LoopDetector -------------------------------------------------------

    #[test]
    fn loop_not_detected_within_limit() {
        let mut det = LoopDetector::default();
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "t".into(),
            arguments: json!({}),
        }];
        // max_strikes = 2: first two calls must succeed
        assert!(det.check(&calls, 2).is_ok());
        assert!(det.check(&calls, 2).is_ok());
    }

    #[test]
    fn loop_detected_on_third_identical_batch_with_max_strikes_2() {
        let mut det = LoopDetector::default();
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "t".into(),
            arguments: json!({}),
        }];
        assert!(det.check(&calls, 2).is_ok());
        assert!(det.check(&calls, 2).is_ok());
        let err = det.check(&calls, 2).unwrap_err();
        assert!(matches!(err, AgentError::LoopDetected { .. }));
    }

    #[test]
    fn different_batches_do_not_trigger_loop() {
        // Two distinct batches should each have independent hit counters.
        // Each can appear up to max_strikes times without triggering.
        let mut det = LoopDetector::default();
        let a = vec![ToolCall {
            id: "c1".into(),
            name: "a".into(),
            arguments: json!({}),
        }];
        let b = vec![ToolCall {
            id: "c2".into(),
            name: "b".into(),
            arguments: json!({}),
        }];
        // max_strikes = 10: each may appear 10 times
        for _ in 0..10 {
            assert!(
                det.check(&a, 10).is_ok(),
                "batch a should not trigger within limit"
            );
            assert!(
                det.check(&b, 10).is_ok(),
                "batch b should not trigger within limit"
            );
        }
        // 11th appearance of `a` should fire (count 11 > 10)
        assert!(
            det.check(&a, 10).is_err(),
            "batch a should trigger on 11th occurrence"
        );
        // `b` is still at count 10 — one more must fire
        assert!(
            det.check(&b, 10).is_err(),
            "batch b should trigger on 11th occurrence"
        );
    }

    #[test]
    fn loop_error_signature_matches_batch_sig() {
        let mut det = LoopDetector::default();
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "x".into(),
            arguments: json!({ "k": "v" }),
        }];
        let expected_sig = batch_signature(&calls);
        det.check(&calls, 0).unwrap_err(); // max_strikes = 0 → first call triggers
        let mut det2 = LoopDetector::default();
        det2.check(&calls, 0).ok(); // ignore first result
        let err = det2.check(&calls, 0).unwrap_err();
        if let AgentError::LoopDetected { signature } = err {
            assert_eq!(signature, expected_sig);
        }
    }

    #[test]
    fn same_name_different_args_do_not_trigger_loop() {
        // Two batches with the same tool name but different arguments must
        // produce distinct signatures and therefore never count as a loop.
        let mut det = LoopDetector::default();
        for i in 0u32..10 {
            let calls = vec![ToolCall {
                id: "c1".into(),
                name: "search".into(),
                arguments: json!({ "q": i }),
            }];
            assert!(
                det.check(&calls, 2).is_ok(),
                "distinct arguments should not trigger loop detection (i={i})"
            );
        }
    }

    #[test]
    fn max_strikes_zero_triggers_on_first_occurrence() {
        // max_strikes = 0 means "no tolerance": even the very first time a
        // batch signature is seen it should be rejected immediately.
        let mut det = LoopDetector::default();
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "instant_tool".into(),
            arguments: json!({}),
        }];
        let err = det
            .check(&calls, 0)
            .expect_err("max_strikes=0 must reject the first occurrence");
        assert!(
            matches!(err, AgentError::LoopDetected { .. }),
            "expected LoopDetected, got {err:?}"
        );
    }
}
