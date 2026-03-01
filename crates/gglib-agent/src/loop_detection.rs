//! Tool-call loop detection via FNV-1a batch signatures.
//!
//! This module is a direct port of the loop-detection logic in
//! `src/hooks/useGglibRuntime/agentLoop.ts`.
//!
//! # Algorithm
//!
//! 1. Compute an **individual signature** for each [`ToolCall`] as
//!    `"{name}:{fnv1a_32(arguments_json):08x}"`.
//! 2. Sort the individual signatures and join them with `"|"` to form a
//!    **batch signature** that is independent of tool-call ordering.
//! 3. A [`LoopDetector`] counts how many times each batch signature has been
//!    seen.  When the count exceeds `max_protocol_strikes` the loop is
//!    considered stuck and [`AgentError::LoopDetected`] is returned.
//!
//! ## Hash algorithm
//!
//! FNV-1a 32-bit with:
//! - Offset basis: `2_166_136_261`
//! - Prime: `16_777_619`
//! - Wrapping 32-bit multiplication (`wrapping_mul`)
//!
//! The Rust implementation hashes **UTF-8 bytes**, matching the JavaScript
//! implementation's behaviour for the ASCII-dominated argument strings
//! produced by OpenAI-compatible tool calls.

use std::collections::HashMap;

use gglib_core::ToolCall;
use gglib_core::ports::AgentError;

use crate::hash::fnv1a_32;

// =============================================================================
// Signature helpers
// =============================================================================

/// Compute the individual signature for a single [`ToolCall`].
///
/// Format: `"{name}:{fnv1a_32(arguments_json):08x}"`
///
/// The arguments are serialised to a canonical JSON string before hashing.
/// For the purposes of loop detection, argument *identity* is what matters,
/// not argument ordering; JSON objects are not canonicalised — this matches
/// the frontend's behaviour.
pub fn tool_signature(call: &ToolCall) -> String {
    let args_json = call.arguments.to_string();
    format!("{}:{:08x}", call.name, fnv1a_32(&args_json))
}

/// Compute the batch signature for a slice of [`ToolCall`]s.
///
/// Individual signatures are sorted before joining so that the result is
/// independent of the order in which the LLM emitted the calls.
pub fn batch_signature(calls: &[ToolCall]) -> String {
    let mut sigs: Vec<String> = Vec::with_capacity(calls.len());
    sigs.extend(calls.iter().map(tool_signature));
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
pub struct LoopDetector {
    hits: HashMap<String, usize>,
}

impl LoopDetector {
    /// Create a fresh detector with empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the current batch of tool calls and error if a loop is detected.
    ///
    /// A loop is declared when the same batch signature has been seen more
    /// than `max_strikes` times.  The count is incremented **before** the
    /// comparison so that `max_strikes = 2` allows two identical batches
    /// before erroring on the third, matching the frontend's
    /// `MAX_SAME_SIGNATURE_HITS = 2` behaviour.
    pub fn check(&mut self, calls: &[ToolCall], max_strikes: usize) -> Result<(), AgentError> {
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
    // Note: fnv1a_32 correctness is covered in crates/gglib-agent/src/hash.rs.

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
            "fs_read:".len() + 8,
            "hash should be 8 hex chars"
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

    // ---- LoopDetector -------------------------------------------------------

    #[test]
    fn loop_not_detected_within_limit() {
        let mut det = LoopDetector::new();
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
        let mut det = LoopDetector::new();
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
        let mut det = LoopDetector::new();
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
        let mut det = LoopDetector::new();
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "x".into(),
            arguments: json!({ "k": "v" }),
        }];
        let expected_sig = batch_signature(&calls);
        det.check(&calls, 0).unwrap_err(); // max_strikes = 0 → first call triggers
        let mut det2 = LoopDetector::new();
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
        let mut det = LoopDetector::new();
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
        let mut det = LoopDetector::new();
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
