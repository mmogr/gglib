//! Stable signatures for a tool-call batch.
//!
//! A batch's identity, for the purpose of asking whether it repeated. Split
//! from the detector because it answers a different question — *is this the
//! same request* — from the one the detector asks, which is *did the same
//! request get the same answer*. See `results.rs` for the other half.

use serde_json::Value;

use super::super::fnv1a::fnv1a_64;
use super::super::tool_types::ToolCall;

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
pub(super) const MAX_REPR_DEPTH: usize = 16;

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
pub(super) fn stable_repr(v: &Value) -> String {
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
/// Arguments are serialised via [`stable_repr`] before hashing so that
/// logically identical arguments always hash identically regardless of JSON
/// key ordering.
pub(super) fn tool_signature(call: &ToolCall) -> String {
    let canonical = stable_repr(&call.arguments);
    format!("{}:{:016x}", call.name, fnv1a_64(&canonical))
}

/// Compute the batch signature for a slice of [`ToolCall`]s.
///
/// Individual signatures are sorted before joining so that the result is
/// independent of the order in which the LLM emitted the calls.
pub fn batch_signature(calls: &[ToolCall]) -> String {
    let mut sigs: Vec<String> = calls.iter().map(tool_signature).collect();
    sigs.sort_unstable();
    sigs.join("|")
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
