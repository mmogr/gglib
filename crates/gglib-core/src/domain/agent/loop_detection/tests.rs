use serde_json::json;

use super::*;
use crate::AgentConfig;

// ---- LoopDetector -----------------------------------------------------------

#[test]
fn loop_not_detected_within_limit() {
    let mut det = LoopDetector::default();
    let calls = vec![ToolCall {
        id: "c1".into(),
        name: "t".into(),
        arguments: json!({}),
    }];
    // max_strikes = 2: first two calls must succeed
    assert!(det.check(&calls, 2, &[], None).is_ok());
    assert!(det.check(&calls, 2, &[], None).is_ok());
}

#[test]
fn loop_detected_on_third_identical_batch_with_max_strikes_2() {
    let mut det = LoopDetector::default();
    let calls = vec![ToolCall {
        id: "c1".into(),
        name: "t".into(),
        arguments: json!({}),
    }];
    assert!(det.check(&calls, 2, &[], None).is_ok());
    assert!(det.check(&calls, 2, &[], None).is_ok());
    let err = det.check(&calls, 2, &[], None).unwrap_err();
    assert!(matches!(err, AgentError::LoopDetected { .. }));
}

#[test]
fn interleaved_batches_never_trigger_a_loop() {
    // This test used to assert the opposite: that A and B kept independent
    // session-wide tallies, and that A was rejected on its 11th occurrence
    // however much work happened in between. That is the wall this change
    // removes. An agent alternating between two pieces of real work is doing
    // exactly what it should, and no number of alternations is evidence of a
    // loop — only repetition with nothing in between is.
    let mut det = LoopDetector::default();
    let a = vec![ToolCall {
        id: "c1".into(),
        name: "a".into(),
        arguments: json!({}),
    }];
    let b = vec![ToolCall {
        id: "c2".into(),
        name: "b".into(),
        arguments: json!({}),
    }];
    // Well past the old session-wide ceiling of 10, and past the tightest
    // threshold the guard ever applies.
    for i in 0..50 {
        assert!(
            det.check(&a, 2, &[], None).is_ok(),
            "alternation {i}: batch a broke b's run and must start over"
        );
        assert!(
            det.check(&b, 2, &[], None).is_ok(),
            "alternation {i}: batch b broke a's run and must start over"
        );
    }
}

#[test]
fn a_run_broken_and_resumed_starts_over() {
    // The counter holds one run, not a per-signature history: returning to a
    // batch after doing something else is a fresh start, not a continuation.
    let mut det = LoopDetector::default();
    let a = vec![ToolCall {
        id: "c1".into(),
        name: "a".into(),
        arguments: json!({}),
    }];
    let b = vec![ToolCall {
        id: "c2".into(),
        name: "b".into(),
        arguments: json!({}),
    }];

    // Two of `a` — one short of the threshold.
    assert!(det.check(&a, 2, &[], None).is_ok());
    assert!(det.check(&a, 2, &[], None).is_ok());
    // `b` breaks the run.
    assert!(det.check(&b, 2, &[], None).is_ok());
    // `a` again is occurrence 1 of a new run, not 3 of the old one.
    assert!(
        det.check(&a, 2, &[], None).is_ok(),
        "a resumed run must start at 1, not continue from 2"
    );
    assert!(det.check(&a, 2, &[], None).is_ok());
    // And the new run trips on its own third. This is the half that keeps the
    // change honest: a reset that restored the allowance without restoring the
    // guard would pass every assertion above and fail here. The unbroken case
    // is `loop_detected_on_third_identical_batch_with_max_strikes_2`.
    assert!(
        det.check(&a, 2, &[], None).is_err(),
        "the resumed run must still trip at its own threshold"
    );
}

#[test]
fn the_batch_that_breaks_a_run_starts_its_own() {
    // The reset arm has to *record* the new signature, not merely forget the
    // old one. Forgetting passes every other test here — the resumed batch
    // starts at 1 either way — and only diverges on the batch that did the
    // breaking, which would silently get one extra strike.
    let mut det = LoopDetector::default();
    let a = vec![ToolCall {
        id: "c1".into(),
        name: "a".into(),
        arguments: json!({}),
    }];
    let b = vec![ToolCall {
        id: "c2".into(),
        name: "b".into(),
        arguments: json!({}),
    }];

    assert!(det.check(&a, 2, &[], None).is_ok(), "a: run of 1");
    // `b` breaks a's run and is itself occurrence 1 of its own.
    assert!(det.check(&b, 2, &[], None).is_ok(), "b: occurrence 1");
    assert!(det.check(&b, 2, &[], None).is_ok(), "b: occurrence 2");
    assert!(
        det.check(&b, 2, &[], None).is_err(),
        "b's third must trip — the breaking batch was recorded, not forgotten"
    );
}

#[test]
fn loop_error_signature_matches_batch_sig() {
    let calls = vec![ToolCall {
        id: "c1".into(),
        name: "x".into(),
        arguments: json!({ "k": "v" }),
    }];
    let expected_sig = batch_signature(&calls);
    // max_strikes = 0 → first occurrence triggers immediately.
    let mut det = LoopDetector::default();
    let err = det.check(&calls, 0, &[], None).unwrap_err();
    if let AgentError::LoopDetected { signature } = err {
        assert_eq!(signature, expected_sig);
    }
}

#[test]
fn same_name_different_args_do_not_trigger_loop() {
    // Two batches with the same tool name but different arguments must
    // produce distinct signatures and therefore never count as a loop.
    let mut det = LoopDetector::default();
    for i in 0u32..10 {
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "search".into(),
            arguments: json!({ "q": i }),
        }];
        assert!(
            det.check(&calls, 2, &[], None).is_ok(),
            "distinct arguments should not trigger loop detection (i={i})"
        );
    }
}

#[test]
fn max_strikes_zero_triggers_on_first_occurrence() {
    // max_strikes = 0 means "no tolerance": even the very first time a
    // batch signature is seen it should be rejected immediately.
    let mut det = LoopDetector::default();
    let calls = vec![ToolCall {
        id: "c1".into(),
        name: "instant_tool".into(),
        arguments: json!({}),
    }];
    let err = det
        .check(&calls, 0, &[], None)
        .expect_err("max_strikes=0 must reject the first occurrence");
    assert!(
        matches!(err, AgentError::LoopDetected { .. }),
        "expected LoopDetected, got {err:?}"
    );
}

// ---- is_observation_batch ---------------------------------------------------

/// Build a single-call batch with the given tool name (no args).
fn obs_batch(name: &str) -> Vec<ToolCall> {
    vec![ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: serde_json::json!({}),
    }]
}

fn patterns(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn is_observation_batch_ends_with_match() {
    // `playwright_mcp_browser_snapshot` ends_with `snapshot` → true.
    let calls = obs_batch("playwright_mcp_browser_snapshot");
    assert!(
        is_observation_batch(&calls, &patterns(&["snapshot"])),
        "ends_with match should return true"
    );
}

#[test]
fn is_observation_batch_contains_match() {
    // `take_screenshot_full` contains `screenshot` → true.
    let calls = obs_batch("take_screenshot_full");
    assert!(
        is_observation_batch(&calls, &patterns(&["screenshot"])),
        "contains match should return true"
    );
}

#[test]
fn is_observation_batch_case_insensitive() {
    // `BROWSER_SNAPSHOT` uppercased must still match pattern `snapshot`.
    let calls = obs_batch("BROWSER_SNAPSHOT");
    assert!(
        is_observation_batch(&calls, &patterns(&["snapshot"])),
        "matching should be case-insensitive"
    );
}

#[test]
fn is_observation_batch_mixed_returns_false() {
    // A batch containing both an observation tool and a non-observation tool
    // must return false — the whole batch falls back to the standard threshold.
    let calls = vec![
        ToolCall {
            id: "c1".into(),
            name: "browser_snapshot".into(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "c2".into(),
            name: "do_thing".into(),
            arguments: serde_json::json!({}),
        },
    ];
    assert!(
        !is_observation_batch(&calls, &patterns(&["snapshot"])),
        "mixed batch (snapshot + do_thing) should return false"
    );
}

#[test]
fn coding_agent_reads_are_observation_tools_by_default() {
    // The regression this arc exists for: a VS Code Copilot / Cline session
    // that reads a file, edits it, then re-reads it to verify was classified
    // as a non-observation repeat and rejected at `max_repeated_batch_steps`
    // (2) instead of `max_observation_steps` (15), because the default
    // pattern list held only browser tool names.
    let defaults = AgentConfig::default().observation_tools;
    for name in [
        // MCP filesystem server.
        "read_file",
        "read_text_file",
        "read_media_file",
        "read_multiple_files",
        "list_directory",
        "list_directory_with_sizes",
        "list_allowed_directories",
        "directory_tree",
        "search_files",
        "get_file_info",
        // Cline / Roo Code.
        "list_files",
        "list_code_definition_names",
        // VS Code Copilot.
        "file_search",
        "grep_search",
        "semantic_search",
        "codebase_search",
        "test_search",
        "get_errors",
        "get_changed_files",
        "get_terminal_output",
        "list_code_usages",
        "fetch_webpage",
        "list_dir",
        // Server-prefixed and vendor-prefixed forms must match too.
        "2:read_file",
        "mcp_filesystem_read_text_file",
    ] {
        assert!(
            is_observation_batch(&obs_batch(name), &defaults),
            "{name} should be an observation tool under the defaults"
        );
    }
}

#[test]
fn observation_patterns_do_not_match_unrelated_names() {
    // Matching is `contains` (which subsumes `ends_with`), so a short
    // fragment would silently
    // exempt unrelated tools from loop detection. This pins the choice to use
    // full tool names: "read" would capture `thread_create`, "list" would
    // capture `listen_port`, and "glob" would capture `set_global_config`.
    let defaults = AgentConfig::default().observation_tools;
    for name in [
        // Fragments that would have captured these: read, list, glob, view.
        "thread_create",
        "listen_port",
        "set_global_config",
        "preview_changes",
        // Mutating tools across the clients gglib serves. Each is a near
        // neighbour of a pattern above and must stay outside the tier.
        "write_file",
        "edit_file",
        "create_file",
        "create_directory",
        "delete_file",
        "delete_directory",
        "move_file",
        "run_in_terminal",
        "run_in_terminal_background",
        "apply_patch",
        "insert_edit_into_file",
        "replace_string_in_file",
        "search_and_replace",
        "execute_command",
        "write_to_file",
    ] {
        assert!(
            !is_observation_batch(&obs_batch(name), &defaults),
            "{name} must not be classified as an observation tool"
        );
    }
}

#[test]
fn a_repeated_file_read_survives_past_the_standard_threshold() {
    // End-to-end through the detector: the same `read_file` batch repeated
    // more times than `max_repeated_batch_steps` allows must still pass,
    // because the observation ceiling applies instead.
    let cfg = AgentConfig::default();
    let calls = obs_batch("read_file");
    let mut detector = LoopDetector::default();

    for i in 1..=cfg.max_repeated_batch_steps.unwrap() + 1 {
        assert!(
            detector
                .check(
                    &calls,
                    cfg.max_repeated_batch_steps.unwrap(),
                    &cfg.observation_tools,
                    cfg.max_observation_steps,
                )
                .is_ok(),
            "repeat {i} of a read_file batch should not trip the guard"
        );
    }
}

#[test]
fn is_observation_batch_empty_patterns_always_false() {
    // An empty pattern list means no tools are ever classified as
    // observation-only — the standard threshold always applies.
    let calls = obs_batch("browser_snapshot");
    assert!(
        !is_observation_batch(&calls, &[]),
        "empty pattern list should always return false"
    );
}

#[test]
fn loop_detector_observation_batch_uses_higher_threshold() {
    // With max_strikes=2 and max_observation_steps=5, an observation-only
    // batch must be allowed up to 5 repetitions without triggering.
    let mut det = LoopDetector::default();
    let calls = obs_batch("playwright_mcp_browser_snapshot");
    let obs_patterns = patterns(&["snapshot"]);
    for _ in 0..5 {
        assert!(
            det.check(&calls, 2, &obs_patterns, Some(5)).is_ok(),
            "observation batch must not trigger within max_observation_steps"
        );
    }
    // 6th occurrence (count = 6 > 5) must fire.
    assert!(
        det.check(&calls, 2, &obs_patterns, Some(5)).is_err(),
        "observation batch must trigger on 6th occurrence"
    );
}

#[test]
fn loop_detector_mixed_batch_uses_standard_threshold() {
    // A mixed batch (snapshot + do_thing) must use max_strikes=2, not the
    // higher observation threshold, even though snapshot is in the list.
    let mut det = LoopDetector::default();
    let mixed = vec![
        ToolCall {
            id: "c1".into(),
            name: "browser_snapshot".into(),
            arguments: serde_json::json!({}),
        },
        ToolCall {
            id: "c2".into(),
            name: "do_thing".into(),
            arguments: serde_json::json!({}),
        },
    ];
    let obs_patterns = patterns(&["snapshot"]);
    assert!(det.check(&mixed, 2, &obs_patterns, Some(10)).is_ok());
    assert!(det.check(&mixed, 2, &obs_patterns, Some(10)).is_ok());
    // 3rd occurrence (count = 3 > 2) must fire — standard threshold applies.
    assert!(
        det.check(&mixed, 2, &obs_patterns, Some(10)).is_err(),
        "mixed batch must use standard threshold (max_strikes=2)"
    );
}
