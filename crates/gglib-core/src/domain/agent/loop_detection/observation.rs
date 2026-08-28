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

// =============================================================================
// Costly-observation classifier
// =============================================================================

/// Shipped observation entries that are read-only *here* but not free to repeat.
///
/// `navigate` changes where the browser session is, `click` changes page state,
/// and `fetch_webpage` spends someone else's rate limit. All three earn their
/// place in the default `observation_tools` list — a browser agent recovering
/// from a redirect, or a coding agent re-reading docs mid-task, is ordinary
/// work. What they cannot have is the *waiver*: the read-only exemption drops
/// the `total` bound entirely, and a fetched page's content essentially always
/// moves, so for these three the exemption is the normal case rather than the
/// exception and the repeat becomes unbounded.
///
/// Kept in the classifier's list and excluded from the waiver, rather than
/// dropped from the list: dropping them would hold a redirect-recovering
/// browser agent to `max_repeated_batch_steps` instead of
/// `max_observation_steps`, which is the regression
/// `test_navigate_tool_uses_elevated_threshold_by_default` exists to catch.
const COSTLY_OBSERVATION_TOOLS: &[&str] = &["navigate", "click", "fetch_webpage"];

/// Return `true` if **any** call in `calls` costs something to repeat.
///
/// Read against the batch's own tool names rather than against the active
/// `observation_tools` list, so a user-supplied list naming one of these is
/// bounded too. The harm is a property of the tool, not of who listed it.
///
/// # Why this does *not* reuse [`is_observation_batch`]'s rule
///
/// The two lists need **opposite** safety properties, so they cannot share a
/// matcher. Over-capturing as observation is *permissive* — it grants a larger
/// allowance — which is why that list can afford `contains` and only has to
/// ensure every captured name is itself read-only. Over-capturing here is
/// *restrictive*: it aborts a session. So this rule leans the other way —
/// it prefers to miss a costly tool over refusing a free one.
///
/// Unanchored `contains` cannot: `click` captures `get_clickable_elements` and
/// `clickhouse_query`, both genuinely read-only, and both would be refused at
/// the 16th call. Matching is therefore anchored to whole `_`/`-`/`.`-delimited
/// segments, which admits `browser_navigate`, `mcp__playwright__click` and
/// `click_element_by_index` while leaving `clickhouse_query` alone. Residue
/// remains — a bare `navigate` segment still catches an LSP-style
/// `navigate_to_definition` — and it is pinned by test rather than hidden: the
/// cost there is a ceiling on repeats, not a refusal of honest work.
///
/// # Two limits, both deliberate
///
/// **Residue.** Anchoring is not exact matching. A bare `navigate` or `click`
/// segment still catches read-only names built from the same word —
/// `navigate_to_definition`, `get_click_count`, `ad_click_report`,
/// `click_house_query`. Those are bounded at 15 rather than refused outright,
/// which is the pre-#928 behaviour and the cheaper of the two errors.
///
/// **camelCase is missed.** `browserNavigate` and `clickElement` carry no
/// separator, so they split to one segment and are *not* recognised as costly
/// — while [`is_observation_batch`]'s `contains` still classifies them, which
/// leaves them exempt and unbounded. Splitting on case boundaries would close
/// that, and would newly capture `clickHouseQuery` — the camelCase spelling of
/// the exact tool the anchoring exists to protect. Given the asymmetry above,
/// missing a bound is the better error than aborting a read-only session, so
/// the gap stays and is pinned by test rather than left to be rediscovered.
/// Narrowing it properly needs the two classifiers to share one anchored rule,
/// which is a change to `observation_tools`' matching and out of scope here.
///
/// `any`, not `all`: one costly call in the batch is enough to spend the thing
/// that must not be spent without bound.
pub(crate) fn is_costly_batch(calls: &[ToolCall]) -> bool {
    calls.iter().any(|call| {
        let name = call.name.to_lowercase();
        let segments: Vec<&str> = split_segments(&name);
        COSTLY_OBSERVATION_TOOLS
            .iter()
            .any(|pat| covers_whole_segments(&segments, pat))
    })
}

/// Split a tool name on the separators clients build compound names from.
fn split_segments(name: &str) -> Vec<&str> {
    name.split(['_', '-', '.'])
        .filter(|s| !s.is_empty())
        .collect()
}

/// Whether `pat` occupies a whole run of `segments`.
///
/// `pat` is split the same way, so a two-word pattern like `fetch_webpage` has
/// to match two consecutive segments rather than appearing inside one.
fn covers_whole_segments(segments: &[&str], pat: &str) -> bool {
    let wanted = split_segments(pat);
    if wanted.is_empty() {
        return false;
    }
    segments
        .windows(wanted.len())
        .any(|w| w == wanted.as_slice())
}

#[cfg(test)]
#[path = "observation_tests.rs"]
mod tests;
