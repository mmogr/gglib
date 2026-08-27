//! Whether a tool-call batch is read-only.
//!
//! Separated from the detector because it answers a different question. The
//! detector counts repeats; this decides which allowance a repeat is held to.
//! Keeping them apart also leaves `mod.rs` room to change.

use crate::ToolCall;

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
/// ([`crate::domain::agent::LoopDetector::check`]) is never invoked with an empty batch — both the
/// agent loop and the proxy's history scan skip loop detection when there are
/// no tool calls. That is now load bearing rather than merely tidy: an empty
/// batch would hash to a signature of its own and break the consecutive run.
pub fn is_observation_batch(calls: &[ToolCall], patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    calls.iter().all(|call| {
        let name = call.name.to_lowercase();
        patterns.iter().any(|pat| {
            // Lowercased here, not at the call sites: patterns are user
            // supplied, and one carrying a capital could never match a name
            // already lowered. That read as case-sensitive; it was a no-op.
            let pat = pat.to_lowercase();
            name.ends_with(&pat) || name.contains(&pat)
        })
    })
}

#[cfg(test)]
#[path = "observation_tests.rs"]
mod tests;
