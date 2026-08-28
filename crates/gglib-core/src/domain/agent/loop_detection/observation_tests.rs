//! Unit tests for [`super::is_observation_batch`] and [`super::is_costly_batch`].

use super::*;
use serde_json::json;

fn call(name: &str) -> Vec<ToolCall> {
    vec![ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: json!({}),
    }]
}

fn patterns(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// The defect. The doc has always said matching is case-insensitive because
/// "both sides are lowercased", but only the tool name was — so a pattern
/// carrying a capital could never match anything, and the configuration was
/// accepted in silence.
///
/// The shape that hits it is ordinary: a VS Code or TypeScript MCP server
/// exposing `readFile` and `listDir`, and a user following the BYO-MCP
/// guidance to name them. Their batches were classified as mutating and held
/// to 2 instead of 15, so read → edit → read-back was refused on the third.
#[test]
fn an_uppercase_pattern_matches() {
    assert!(
        is_observation_batch(&call("readFile"), &patterns(&["readFile"])),
        "a camelCase pattern must classify the tool it names"
    );
    assert!(
        is_observation_batch(&call("mcp__fs__listDir"), &patterns(&["listDir"])),
        "and must still match as a suffix"
    );
}

/// Case-insensitive in both directions, not merely tolerant of one.
#[test]
fn case_does_not_matter_on_either_side() {
    for (name, pat) in [
        ("READ_FILE", "read_file"),
        ("read_file", "READ_FILE"),
        ("ReAd_FiLe", "rEaD_fIlE"),
    ] {
        assert!(
            is_observation_batch(&call(name), &patterns(&[pat])),
            "{name} should match {pat}"
        );
    }
}

/// The ordinary path is untouched — this is a widening, and a widening that
/// changed the lowercase case would be a different bug.
#[test]
fn lowercase_patterns_behave_exactly_as_before() {
    assert!(is_observation_batch(
        &call("read_file"),
        &patterns(&["read_file"])
    ));
    assert!(is_observation_batch(
        &call("mcp_read_file"),
        &patterns(&["read_file"])
    ));
    assert!(!is_observation_batch(
        &call("write_file"),
        &patterns(&["read_file"])
    ));
}

/// A batch is observation-only when *every* call is, and an empty pattern list
/// classifies nothing. Both predate this change; pinned here because they are
/// the properties a careless normalisation would break.
#[test]
fn the_all_and_empty_rules_survive() {
    let mixed = vec![call("read_file").remove(0), call("write_file").remove(0)];
    assert!(!is_observation_batch(&mixed, &patterns(&["read_file"])));
    assert!(!is_observation_batch(&call("read_file"), &[]));
}

// =============================================================================
// Costly-observation classifier
// =============================================================================

/// The three shipped entries whose repeat is not free, reached under the
/// compound names clients actually build.
#[test]
fn the_costly_defaults_are_recognised_under_real_client_names() {
    for name in [
        "fetch_webpage",
        "browser_navigate",
        "playwright_browser_click",
        "mcp__playwright__click",
        "BROWSER_NAVIGATE",
        "click_element_by_index",
        "double_click_element",
        "navigate_to_url",
    ] {
        assert!(is_costly_batch(&call(name)), "{name} is not free to repeat");
    }
}

/// The mirror of `observation_patterns_do_not_match_unrelated_names`, and the
/// reason the match is segment-anchored rather than `contains`.
///
/// Over-capturing here *aborts a session*, which is the opposite of what
/// over-capturing in `observation_tools` does, so this list cannot afford the
/// substring rule its sibling can. Every name below is genuinely read-only,
/// is classified as an observation batch by the shipped default list, and was
/// refused at the 16th call while the match was unanchored.
#[test]
fn read_only_tools_that_merely_contain_a_costly_word_keep_the_waiver() {
    for name in [
        "get_clickable_elements",
        "browser_get_clickable_elements",
        "clickhouse_query",
    ] {
        assert!(
            !is_costly_batch(&call(name)),
            "{name} changes nothing anywhere and must keep the waiver"
        );
    }
}

/// camelCase names are missed, and that is the accepted side of the trade.
///
/// They carry no separator, so they split to one segment. `is_observation_batch`
/// still classifies them by `contains`, so they stay exempt and unbounded —
/// the hole #944 exists to close, left open for this spelling. Closing it by
/// splitting on case boundaries would newly capture `clickHouseQuery`, and
/// refusing a read-only session is the worse error. Pinned so the limit is a
/// decision rather than a surprise.
#[test]
fn camel_case_names_are_missed_which_is_the_accepted_side_of_the_trade() {
    for name in ["browserNavigate", "clickElement", "fetchWebpage"] {
        assert!(
            !is_costly_batch(&call(name)),
            "{name}: if this now passes, the camelCase gap was closed and the \
             clickHouseQuery trade needs re-deciding"
        );
    }
}

/// Segment anchoring is not exact matching, and this is what it still catches.
///
/// A bare `navigate` segment cannot tell a browser tool from a code-navigation
/// one, so an LSP-style `navigate_to_definition` is bounded at 15 despite being
/// read-only. Recorded rather than hidden: the cost is a ceiling on repeats —
/// the pre-#928 behaviour — not a refusal of honest work, and narrowing the
/// pattern to `browser_navigate` would stop matching the bare `navigate` the
/// shipped `observation_tools` list actually carries.
#[test]
fn read_only_names_built_from_a_costly_word_are_bounded_too() {
    for name in [
        "navigate_to_definition",
        "get_click_count",
        "ad_click_report",
        "click_house_query",
    ] {
        assert!(
            is_costly_batch(&call(name)),
            "{name} is residue, not a target"
        );
    }
}

/// The read-only entries the waiver was actually argued for keep it.
#[test]
fn ordinary_read_only_tools_are_not_costly() {
    for name in [
        "read_file",
        "list_dir",
        "grep_search",
        "semantic_search",
        "get_errors",
    ] {
        assert!(!is_costly_batch(&call(name)), "{name} is free to repeat");
    }
}

/// `any`, not `all`: one costly call spends the thing that must not be spent
/// without bound, whatever it is batched with.
#[test]
fn one_costly_call_makes_the_whole_batch_costly() {
    let mut mixed = call("read_file");
    mixed.extend(call("fetch_webpage"));
    assert!(is_costly_batch(&mixed));

    let mut free = call("read_file");
    free.extend(call("list_dir"));
    assert!(!is_costly_batch(&free));

    assert!(!is_costly_batch(&[]), "an empty batch spends nothing");
}

/// The patterns are compared against an already-lowercased name, so an entry
/// carrying a capital would be a silent no-op — the exact defect #932 fixed
/// for [`super::is_observation_batch`]. Pinned rather than left to discipline.
#[test]
fn every_costly_pattern_is_lowercase() {
    for pat in super::COSTLY_OBSERVATION_TOOLS {
        assert_eq!(
            *pat,
            pat.to_lowercase(),
            "{pat} could never match a lowercased name"
        );
    }
}
