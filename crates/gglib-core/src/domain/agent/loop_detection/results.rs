//! Hashing the answers a tool-call batch received, for equality alone.
//!
//! [`super::LoopDetector`] decides whether a repeat is a strike by asking
//! whether the answer changed, and two callers have to ask it the same way:
//! the agent loop, which holds `ToolResult`s it produced itself, and the
//! proxy, which reconstructs the pairing from a replayed transcript. The
//! *rule* — pair each call with its own answer, sort the pairs, hash — lives
//! here so there is one of it. The *sourcing* stays with each caller, because
//! that is where they genuinely differ: the proxy has to bound itself to the
//! contiguous run of `role: "tool"` messages after an assistant turn, which
//! is a wire-format concern the agent loop does not have.
//!
//! # Why `DefaultHasher` and not the `fnv1a_64` next door
//!
//! Equality is the only property used. These values are compared within one
//! process, never persisted, never sent, and never shown to anyone —
//! `AgentError::LoopDetected` carries the *signature*, which is FNV-1a hex, and
//! is a different thing. `DefaultHasher` is unspecified across Rust releases,
//! which would matter if any of that were untrue and does not. Keeping it is
//! also what makes moving this code out of `gglib-proxy` provably behaviour
//! preserving rather than merely intended to be.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::Value;

use super::super::tool_types::ToolCall;
use super::signature::stable_repr;

// =============================================================================
// One answer
// =============================================================================

/// Hash one answer's text.
///
/// The `Value::String` arm of [`hash_result_content`], without building a
/// `Value` to hold a string the caller already has. Tool results run to tens
/// of kilobytes and the proxy hashes them on a pre-admission path, so the
/// copy is worth avoiding.
#[must_use]
pub fn hash_result_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (0u8, text).hash(&mut hasher);
    hasher.finish()
}

/// Hash one answer's content, whatever shape it arrived in.
///
/// Deliberately *not* a text projection: projecting objects, numbers and nulls
/// to the empty string would make two different structured results compare
/// equal, which manufactures an "identical" repeat out of nothing.
///
/// The leading discriminant is why `null` and the string `"null"` cannot
/// collide, in a function whose only job is equality.
#[must_use]
pub fn hash_result_content(content: &Value) -> u64 {
    match content {
        Value::String(s) => hash_result_text(s),
        other => {
            let mut hasher = DefaultHasher::new();
            (1u8, other.to_string()).hash(&mut hasher);
            hasher.finish()
        }
    }
}

// =============================================================================
// One batch
// =============================================================================

/// Hash the answers to one batch of tool calls.
///
/// `answers[i]` is the hash of the answer to `calls[i]`, or `None` if that
/// call went unanswered. Returns `None` when any call is unanswered, or when
/// the two slices disagree in length: a partially-answered batch says nothing
/// about whether work repeated, and neither does a caller that has lost track
/// of which answer belongs to which call.
///
/// **Pairs, not bare hashes.** Sorting answer hashes alone would meet the
/// ordering goal — [`super::batch_signature`] sorts too, so the same parallel
/// batch re-emitted in a different order must still match — but it severs
/// which call produced which result, and a two-call batch whose answers
/// swapped between occurrences would compare equal.
///
/// The pair key canonicalises `arguments` through
/// [`super::signature::stable_repr`] — the *same* rendering
/// [`super::batch_signature`] uses, and that is load bearing rather than tidy.
/// `stable_repr` collapses everything below `MAX_REPR_DEPTH` to a sentinel, so
/// a bare `Value::to_string` here would distinguish batches the signature calls
/// identical: one run, a different answers hash every occurrence, and a rescue
/// that never ends. An observation-tier batch built that way could never be
/// refused at all — which inverts the depth cap's own safety argument, that a
/// collision can only ever make the guard *stricter*.
///
/// It also removes a dependence the previous rendering carried on
/// `serde_json::Value` being a `BTreeMap`: `stable_repr` sorts keys itself, so
/// enabling `preserve_order` cannot make this join quietly under-report.
#[must_use]
pub fn batch_results_hash(calls: &[ToolCall], answers: &[Option<u64>]) -> Option<u64> {
    if answers.len() != calls.len() {
        return None;
    }
    // `collection_is_never_read` does not count `Hash::hash` as a read, and
    // `keyed` is read by exactly that, two lines below. The lint is a nursery
    // one and this crate inherits the workspace's nursery set while
    // `gglib-proxy`, where this code used to live, does not — so the same
    // lines passed there and fail here. Reproducing `Vec::hash` by hand to
    // satisfy it would mean relying on `write_length_prefix`'s default being
    // `write_usize`, which is a subtler equivalence than the one it buys.
    #[allow(clippy::collection_is_never_read)]
    let mut keyed: Vec<(String, u64)> = calls
        .iter()
        .zip(answers)
        .map(|(call, answer)| {
            Some((
                format!("{}\u{0}{}", call.name, stable_repr(&call.arguments)),
                (*answer)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    keyed.sort_unstable();

    let mut hasher = DefaultHasher::new();
    keyed.hash(&mut hasher);
    Some(hasher.finish())
}

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
