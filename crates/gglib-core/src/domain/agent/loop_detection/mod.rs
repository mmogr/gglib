#![doc = include_str!("README.md")]
#[cfg(test)]
mod tests;

use serde_json::Value;

use super::fnv1a::fnv1a_64;
use super::tool_types::ToolCall;
use crate::ports::AgentError;

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
/// Arguments are serialised via [`stable_repr`] before hashing so that
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
pub fn batch_signature(calls: &[ToolCall]) -> String {
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
/// ([`LoopDetector::check`]) is never invoked with an empty batch — both the
/// agent loop and the proxy's history scan skip loop detection when there are
/// no tool calls. That is now load bearing rather than merely tidy: an empty
/// batch would hash to a signature of its own and break the consecutive run.
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

/// Stateful guard that detects when the same tool-call batch repeats **back to
/// back**.
///
/// Create once per agent run and call [`LoopDetector::check`] after every
/// iteration that produces tool calls.
///
/// Counting is run-length, not session-wide: only the current unbroken run of
/// one signature is held, and a different batch discards it. A session-wide
/// tally made any long conversation terminal — a client replays the whole
/// history every turn, so a batch that recurred often enough anywhere in the
/// session was rejected on every subsequent request for the rest of it. The
/// case that made it urgent is ordinary work rather than a loop: an agent that
/// runs one command, edits, runs it again, edits again and runs it a third
/// time reaches three occurrences of an identical batch well inside a normal
/// task, and `max_repeated_batch_steps` is 2.
///
/// The cost is that a *cycle* of tool batches is no longer caught, at any
/// period of two or more — A → B → A → B, and equally A → A → B repeating,
/// where the run reaches the threshold on every pass without ever crossing
/// it. That is accepted rather than worked around: separating a cycle from
/// the scattered repeats above needs a window or a decay rate, and there is
/// no measurement behind either number.
///
/// Nothing backstops it in the general case, and saying otherwise would be
/// worse than the gap. [`super::StagnationDetector`] keeps its session-wide
/// counting and catches an oscillating session *only if the model also
/// repeats its prose* — and a tool-call-only turn carries `content: null`,
/// which that detector ignores by design. So a model alternating two batches
/// and narrating nothing is now refused by neither guard. What observes it is
/// `identical_result_repeats` in the proxy's ledger, which is a reading for a
/// person and not a verdict.
#[derive(Debug, Default)]
pub struct LoopDetector {
    /// The signature of the most recent batch, and how many times in a row it
    /// has now been seen. `None` until the first batch arrives.
    run: Option<(String, usize)>,
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
    /// The run length is incremented **before** the comparison so that
    /// `effective_max = 2` allows two identical batches before erroring on
    /// the third.
    ///
    /// `effective_max = 0` rejects the very first occurrence (zero tolerance).
    ///
    /// A batch with a different signature resets the run to 1 — and that is
    /// the only thing that resets it. Both call sites skip this method when
    /// the batch is empty, so a prose answer, a `role: "tool"` result and a
    /// user interjection all pass without breaking a run. That is load
    /// bearing rather than incidental: every real tool call is answered by a
    /// result message before the next one, so a run broken by anything other
    /// than a different batch could never reach two.
    pub fn check(
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
        let count = match &mut self.run {
            Some((last, count)) if *last == sig => {
                *count += 1;
                *count
            }
            slot => {
                *slot = Some((sig.clone(), 1));
                1
            }
        };
        if count > effective_max {
            return Err(AgentError::LoopDetected { signature: sig });
        }
        Ok(())
    }
}
