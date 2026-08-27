//! Unit tests for [`super::is_observation_batch`].

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
