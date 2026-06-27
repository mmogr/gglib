#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Tool-call loop detection via FNV-1a batch signatures.
//!
//! # Algorithm
//!
//! 1. Compute an **individual signature** for each [`ToolCall`] as
//!    `"{name}:{fnv1a_64(canonical_args_json):016x}"`.
//! 2. Sort the individual signatures and join them with `"|"` to form a
//!    **batch signature** that is independent of tool-call ordering.
//! 3. A [`LoopDetector`] counts how many times each batch signature has been
//!    seen.  The threshold applied depends on whether the batch is classified
//!    as "observation-only" (see below).
//!
//! # Dual-threshold detection
//!
//! Observation-only tools (e.g. browser snapshots, page screenshots) take no
//! meaningful arguments, so every call hashes to the same signature regardless
//! of the page content returned.  With a strict threshold this causes false
//! positives on legitimate `ReAct` *observe → act → observe* cycles.
//!
//! The detector therefore applies **two thresholds**:
//!
//! | Batch type | Threshold used |
//! |------------|---------------|
//! | Every call matches an observation pattern | `max_observation_steps` |
//! | At least one call does **not** match | `max_repeated_batch_steps` |
//!
//! A batch is observation-only when [`is_observation_batch`] returns `true`:
//! every call's lowercased name satisfies
//! `name.ends_with(pattern) || name.contains(pattern)` for at least one
//! pattern in the configured list.  Substring/suffix matching is used
//! intentionally so that namespaced MCP tool names such as
//! `playwright_mcp_browser_snapshot` are matched by the short pattern
//! `"snapshot"` without requiring users to enumerate every vendor variant.
//!
//! **Mixed batches** (≥ 1 non-observation call) always fall back to the
//! stricter `max_repeated_batch_steps` — the conservative choice.
//!
//! # Hash algorithm
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
// Observation-batch classifier
// =============================================================================

/// Return `true` if **every** call in `calls` is an observation-only tool.
///
/// A tool call is classified as observation-only when its lowercased name
/// satisfies `name.ends_with(pattern) || name.contains(pattern)` for at
/// least one pattern in `patterns`.  Matching is case-insensitive (both
/// sides are lowercased before comparison).
///
/// An empty `patterns` list means no tools are ever classified as
/// observation-only, so the function always returns `false`.
///
/// An empty `calls` slice returns `true` (vacuous truth), but the caller
/// ([`LoopDetector::check`]) is never invoked with an empty batch — the
/// agent loop skips loop detection when there are no tool calls.
pub fn is_observation_batch(calls: &[ToolCall], patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    calls.iter().all(|call| {
        let name = call.name.to_lowercase();
        patterns
            .iter()
            .any(|pat| name.ends_with(pat.as_str()) || name.contains(pat.as_str()))
    })
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
    /// Selects the effective threshold based on batch classification:
    ///
    /// - If every call in `calls` matches an observation pattern in
    ///   `observation_tools` (via [`is_observation_batch`]), `max_observation_steps`
    ///   is used as the threshold (falling back to `max_strikes` when `None`).
    /// - Otherwise, `max_strikes` (`max_repeated_batch_steps`) is used.
    ///
    /// The count is incremented **before** the comparison so that
    /// `effective_max = 2` allows two identical batches before erroring on
    /// the third.
    ///
    /// `effective_max = 0` rejects the very first occurrence (zero tolerance).
    pub(crate) fn check(
        &mut self,
        calls: &[ToolCall],
        max_strikes: usize,
        observation_tools: &[String],
        max_observation_steps: Option<usize>,
    ) -> Result<(), AgentError> {
        let effective_max = if is_observation_batch(calls, observation_tools) {
            max_observation_steps.unwrap_or(max_strikes)
        } else {
            max_strikes
        };
        let sig = batch_signature(calls);
        let entry = self.hits.entry(sig.clone()).or_insert(0);
        *entry += 1;
        let count = *entry;
        if count > effective_max {
            return Err(AgentError::LoopDetected { signature: sig });
        }
        Ok(())
    }
}
