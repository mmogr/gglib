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

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gglib_core::ToolCall;
use gglib_core::ports::AgentError;
use serde_json::Value;

use crate::fnv1a::fnv1a_64;

// =============================================================================
// Signature helpers
// =============================================================================

/// Maximum recursion depth for [`stable_repr_inner`].
///
/// Deeply-nested JSON arguments (e.g. from a hostile tool result fed back
/// into tool arguments) would otherwise cause unbounded stack growth.  Values
/// beyond this depth are replaced with the sentinel `"..."`.
///
/// **Truncation impact on hashing**: values deeper than this limit are
/// collapsed to the same sentinel string, meaning structurally distinct
/// deeply-nested arguments will produce **identical hashes**.  This is
/// acceptable because the loop detector is a best-effort guard — a false
/// positive (treating distinct deep arguments as a loop) is safe (it aborts
/// the run), while a false negative cannot occur for shallow arguments which
/// represent the vast majority of real tool calls.
const MAX_REPR_DEPTH: usize = 16;

/// Produce a **deterministic string representation** of a [`serde_json::Value`]
/// suitable for stable hashing.
///
/// Object keys are sorted recursively so that `{"b":2,"a":1}` and
/// `{"a":1,"b":2}` produce identical output.  Array element order is
/// preserved.  Recursion is capped at [`MAX_REPR_DEPTH`] to prevent
/// stack overflow on adversarially nested inputs; values beyond that depth
/// are replaced with the sentinel `"..."`, which means two deeply-nested
/// values that differ only below depth 16 will hash identically.
///
/// The output is **not** valid JSON — it is intentionally compact and only
/// used as a pre-image for FNV-1a; never parsed or returned to callers.
fn stable_repr(v: &Value) -> String {
    stable_repr_inner(v, 0)
}

fn stable_repr_inner(v: &Value, depth: usize) -> String {
    if depth >= MAX_REPR_DEPTH {
        return "\"...\"".to_owned();
    }
    match v {
        Value::Object(map) => {
            let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
            pairs.sort_unstable_by_key(|(k, _)| k.as_str());
            let inner = pairs
                .into_iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k)
                            .expect("in-memory String serialisation is infallible"),
                        stable_repr_inner(v, depth + 1)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Value::Array(arr) => {
            let inner = arr
                .iter()
                .map(|e| stable_repr_inner(e, depth + 1))
                .collect::<Vec<_>>()
                .join(",");
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
    let canonical = stable_repr(&call.arguments);
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
pub struct LoopDetector {
    hits: HashMap<String, usize>,
}

impl LoopDetector {
    /// Record the current batch of tool calls and error if a loop is detected.
    ///
    /// A loop is declared when the same batch signature has been seen more
    /// than `max_strikes` times.  The count is incremented **before** the
    /// comparison so that `max_strikes = 2` allows two identical batches
    /// before erroring on the third.
    ///
    /// `max_strikes = 0` rejects the very first occurrence (zero tolerance).
    /// `max_strikes = 1` rejects on the second occurrence (one repeat allowed).
    pub(crate) fn check(
        &mut self,
        calls: &[ToolCall],
        max_strikes: usize,
    ) -> Result<(), AgentError> {
        let sig = batch_signature(calls);
        let entry = self.hits.entry(sig.clone()).or_insert(0);
        *entry += 1;
        let count = *entry;
        if count > max_strikes {
            return Err(AgentError::LoopDetected { signature: sig });
        }
        Ok(())
    }
}
