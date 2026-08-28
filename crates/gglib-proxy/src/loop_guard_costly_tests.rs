//! Observation tools that are read-only *here* but not free to repeat.
//!
//! A fifth test module because `loop_guard_tests.rs` is frozen at its current
//! size by the complexity ratchet. Helpers are duplicated for the same reason.
//!
//! These are the probes from [#944](https://github.com/mmogr/gglib/issues/944),
//! turned into regressions. Before the costly split they all read `Pass` at 60
//! occurrences, because #928's waiver removed the `total` bound outright and a
//! fetched page's content moves on every call — so the exemption was the normal
//! case rather than the exception.
//!
//! The proxy path is the one that matters here: it has no `max_iterations`, so
//! an unbounded repeat is bounded only by the client noticing.

use super::*;
use serde_json::json;

fn cfg() -> LoopGuardConfig {
    LoopGuardConfig::from_settings(&Settings::with_defaults()).expect("guard on by default")
}

fn body(messages: &[Value]) -> Vec<u8> {
    json!({ "model": "m", "messages": messages })
        .to_string()
        .into_bytes()
}

/// `n` occurrences of one batch calling `tool`, each answered differently.
fn moving_calls(tool: &str, n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|i| {
            [
                json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": format!("c{i}"),
                        "type": "function",
                        "function": { "name": tool, "arguments": "{}" }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": format!("c{i}"), "content": format!("{i} changed") }),
            ]
        })
        .collect()
}

fn verdict_for(tool: &str, n: usize) -> LoopGuardVerdict {
    scan_history(&body(&moving_calls(tool, n)), &cfg()).verdict
}

/// `n` occurrences of one batch calling `tool`, every one answered identically.
///
/// The distinction matters: with *moving* answers `record_results` resets
/// `count`, so `count > effective_max` can never fire and the classification
/// tier is invisible to the outcome. Only identical answers exercise it.
fn repeated_calls(tool: &str, n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|i| {
            [
                json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": format!("c{i}"),
                        "type": "function",
                        "function": { "name": tool, "arguments": "{}" }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": format!("c{i}"), "content": "same" }),
            ]
        })
        .collect()
}

fn verdict_repeated(tool: &str, n: usize) -> LoopGuardVerdict {
    scan_history(&body(&repeated_calls(tool, n)), &cfg()).verdict
}

/// The three shipped entries the waiver should never have covered.
///
/// `fetch_webpage` spends someone else's rate limit, `navigate` moves the
/// browser session, `click` changes page state. Each is reached through the
/// same substring rule that classifies it, so the realistic client-side names
/// are used rather than the bare patterns.
#[test]
fn a_costly_observation_batch_is_refused_at_the_ceiling() {
    for tool in [
        "fetch_webpage",
        "browser_navigate",
        "mcp__playwright__click",
    ] {
        assert_eq!(
            verdict_for(tool, 15),
            LoopGuardVerdict::Pass,
            "{tool}: the allowance itself is unchanged"
        );
        assert!(
            matches!(verdict_for(tool, 16), LoopGuardVerdict::LoopDetected { .. }),
            "{tool}: the 16th moving occurrence must be refused"
        );
    }
}

/// The issue's probe, at its own numbers. Sixty consecutive calls with a
/// different answer every time used to reach `Pass`.
#[test]
fn sixty_moving_costly_calls_no_longer_pass() {
    for tool in ["fetch_webpage", "browser_click", "browser_navigate"] {
        assert!(
            matches!(verdict_for(tool, 60), LoopGuardVerdict::LoopDetected { .. }),
            "{tool}: 60 moving occurrences must not pass"
        );
    }
}

/// The half that must not change, and the reason this is Option 3 rather than
/// dropping the three names from `observation_tools`.
///
/// A costly tool is still *classified* as observation, so it keeps
/// `max_observation_steps` (15) rather than falling back to
/// `max_repeated_batch_steps` (2). Answers must be **identical** for this to
/// mean anything: a moving answer resets `count`, so the tier would be
/// invisible and the assertion would hold whether or not the tool was
/// classified — which is exactly how the first version of this test managed to
/// pass against unmodified `main`.
///
/// With identical answers it fails at occurrence 3 if `navigate` is dropped
/// from the list, which is the redirect-recovery regression
/// `test_navigate_tool_uses_elevated_threshold_by_default` exists to catch.
#[test]
fn a_costly_tool_keeps_the_elevated_threshold_it_was_classified_for() {
    assert_eq!(
        verdict_repeated("browser_navigate", 15),
        LoopGuardVerdict::Pass,
        "still on the observation tier, not the 2-strike one"
    );
    assert!(
        matches!(
            verdict_repeated("browser_navigate", 16),
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "and the tier's own ceiling still ends it"
    );
}

/// The waiver survives for tools it was actually argued for.
///
/// `read_file` changes nothing on this machine or anyone else's, so repeating
/// it stays free however far the answers move. This mirrors
/// `an_observation_run_rescued_by_new_answers_has_no_ceiling` at the proxy
/// level, and is what a narrower fix would have broken.
#[test]
fn a_genuinely_read_only_batch_still_has_no_ceiling() {
    assert_eq!(
        verdict_for("read_file", 100),
        LoopGuardVerdict::Pass,
        "a read-only repeat with moving answers is still unbounded"
    );
}

/// Control: a mutating batch was already bounded and is untouched.
#[test]
fn a_mutating_batch_is_bounded_exactly_as_before() {
    assert_eq!(verdict_for("write_file", 15), LoopGuardVerdict::Pass);
    assert!(matches!(
        verdict_for("write_file", 16),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}
