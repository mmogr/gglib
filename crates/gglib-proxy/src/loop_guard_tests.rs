//! Unit tests for [`super`]: the pre-dispatch loop and stagnation guard.
//!
//! Split out of `loop_guard.rs` to keep that file under the repo's file-size
//! ratchet; see `scripts/check_rust_complexity.sh`.

use super::*;
use serde_json::json;

fn cfg() -> LoopGuardConfig {
    LoopGuardConfig::from_settings(&Settings::with_defaults()).expect("guard on by default")
}

/// Build a request body from raw message values.
fn body(messages: &[Value]) -> Vec<u8> {
    json!({ "model": "m", "messages": messages })
        .to_string()
        .into_bytes()
}

fn assistant_call(name: &str, args: &str) -> Value {
    json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": { "name": name, "arguments": args }
        }]
    })
}

fn assistant_text(text: &str) -> Value {
    json!({ "role": "assistant", "content": text })
}

/// The verdict alone, for the many cases that predate [`ScanOutcome`].
fn verdict_of(body: &[u8], cfg: &LoopGuardConfig) -> LoopGuardVerdict {
    scan_history(body, cfg).verdict
}

/// An assistant turn with one tool call carrying an explicit id.
fn assistant_call_id(id: &str, name: &str, args: &str) -> Value {
    json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": id,
            "type": "function",
            "function": { "name": name, "arguments": args }
        }]
    })
}

/// The environment's answer to `id`.
fn tool_result(id: &str, content: &str) -> Value {
    json!({ "role": "tool", "tool_call_id": id, "content": content })
}

// ── Pass cases ───────────────────────────────────────────────────────────

#[test]
fn empty_and_first_turn_bodies_pass() {
    assert_eq!(verdict_of(&body(&[]), &cfg()), LoopGuardVerdict::Pass);
    let first_turn = body(&[
        json!({ "role": "system", "content": "be helpful" }),
        json!({ "role": "user", "content": "hi" }),
    ]);
    assert_eq!(verdict_of(&first_turn, &cfg()), LoopGuardVerdict::Pass);
}

#[test]
fn unparseable_body_fails_open() {
    assert_eq!(
        verdict_of(b"not json at all", &cfg()),
        LoopGuardVerdict::Pass
    );
    // messages of the wrong shape entirely — still a pass, not a panic.
    assert_eq!(
        verdict_of(br#"{"messages": "nope"}"#, &cfg()),
        LoopGuardVerdict::Pass
    );
}

#[test]
fn non_assistant_roles_are_ignored() {
    // Identical tool results and user messages must never count.
    let msgs: Vec<Value> = (0..10)
        .flat_map(|_| {
            vec![
                json!({ "role": "user", "content": "same" }),
                json!({ "role": "tool", "content": "same result", "tool_call_id": "c1" }),
            ]
        })
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

#[test]
fn two_identical_batches_pass_at_default_threshold() {
    // A mutating tool: held to `max_repeated_batch_steps` (2), so two
    // occurrences is the last passing case and the test can still fail.
    let msgs = vec![
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
    ];
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

#[test]
fn distinct_arguments_never_trip() {
    // Must be a *mutating* tool for this to mean anything: under an
    // observation ceiling of 15, ten batches pass whether or not
    // canonicalisation collapsed them, and the test could not fail.
    let msgs: Vec<Value> = (0..10)
        .map(|i| assistant_call("write_file", &format!(r#"{{"path":"file{i}.rs"}}"#)))
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

// ── Loop detection ───────────────────────────────────────────────────────

#[test]
fn third_identical_batch_trips_loop() {
    // A mutating tool: not in `observation_tools`, so the standard
    // `max_repeated_batch_steps` threshold of 2 applies.
    let msgs = vec![
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn a_batch_repeated_with_real_work_in_between_never_trips() {
    // The wall this change removes, at the layer a client meets it. Running
    // one command, editing, and running it again is ordinary work, and it
    // reached three identical `write_file` batches — the standard threshold —
    // well inside a normal task. Fifty alternations is far past any ceiling
    // the guard applies, and none of them is evidence of a loop.
    //
    // This also pins the deliberately accepted cost: A -> B -> A -> B on tool
    // batches is no longer caught at all. If that is ever revisited, this
    // test is the one that must be argued with.
    let msgs: Vec<Value> = (0..50)
        .flat_map(|_| {
            vec![
                assistant_call("write_file", r#"{"path":"a.rs"}"#),
                assistant_call("run_tests", r#"{"suite":"unit"}"#),
            ]
        })
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

#[test]
fn a_cycle_that_never_exceeds_the_threshold_escapes_entirely() {
    // The accepted cost, stated at its real width. It is not only strict
    // alternation: run-length counting sees an unbroken run, so *any* cycle
    // whose period is two or more escapes forever, including one that reaches
    // the threshold on every pass without ever crossing it.
    //
    // This shape is a genuinely stuck agent — running the same failing test
    // twice, editing nothing that helps, and doing it again. It was rejected
    // on the third turn before this change and is never rejected now. The
    // session-wide tally that caught it is the same one that rejected an agent
    // for running `cargo test` three times across an hour of real work, which
    // is why it went; but the trade is real in both directions and this is the
    // half that costs something.
    //
    // `identical_result_repeats` is what observes it. If that counter reads
    // high in real use, this test is the one to argue with.
    //
    // Reading the answers did not close this. The run breaks on *signature*
    // before any answer is consulted, so a cycle escapes whatever came back —
    // and this history carries no `role: "tool"` messages at all, so it is
    // unjoinable on top of that. Both are true independently; fixing either
    // would leave the other. See ADR 0010.
    let msgs: Vec<Value> = (0..40)
        .flat_map(|_| {
            vec![
                assistant_call("run_tests", r#"{"suite":"unit"}"#),
                assistant_call("run_tests", r#"{"suite":"unit"}"#),
                assistant_call("write_file", r#"{"path":"a.rs"}"#),
            ]
        })
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

#[test]
fn prose_and_results_between_identical_batches_do_not_break_the_run() {
    // Load bearing, and the reason the run is broken only by a *different
    // batch*. Every real tool call is answered by a `role: "tool"` message
    // before the next one, and a model often narrates in between, so a run
    // that either of those could break would never reach two and the guard
    // would never fire on a real conversation at all.
    //
    // Moving `loops.check` out from behind its `!calls.is_empty()` guard
    // looks like a tightening and would silently disable the guard instead.
    // This test is what fails if someone does.
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "1 file changed"),
        assistant_text("that did not work, let me try again"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "1 file changed"),
        assistant_text("still not right"),
        assistant_call_id("c3", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c3", "1 file changed"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn a_user_interjection_between_identical_batches_does_not_break_the_run() {
    // Same property for the `_ =>` arm of the role match: a user turn ends
    // the *observation* the two counters report, but it is not work, so it
    // does not restore the model's allowance to repeat itself.
    let msgs = vec![
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
        json!({ "role": "user", "content": "keep going" }),
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
        assistant_call("write_file", r#"{"path":"a.rs"}"#),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn a_thrice_read_file_does_not_trip_the_loop_guard() {
    // This case used to be `third_identical_batch_trips_loop`, asserting
    // the opposite. Reading the same file three times — read, edit,
    // re-read to verify — is the ordinary shape of an agentic coding
    // turn, and rejecting it killed the conversation permanently: the
    // client replays the same history every turn, so the 400 repeats
    // forever and its body tells a non-technical user to run a CLI flag.
    //
    // `read_file` is an observation tool, so the ceiling is 15, not 2.
    let msgs = vec![
        assistant_call("read_file", r#"{"path":"a.rs"}"#),
        assistant_call("read_file", r#"{"path":"a.rs"}"#),
        assistant_call("read_file", r#"{"path":"a.rs"}"#),
    ];
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

// ── Tool results: observed, never decisive ───────────────────────────────

#[test]
fn a_repeat_that_got_the_same_answer_is_reported() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "error: cannot borrow `x` as mutable"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "error: cannot borrow `x` as mutable"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert_eq!(outcome.verdict, LoopGuardVerdict::Pass, "under threshold");
    assert!(outcome.identical_result_repeat);
}

#[test]
fn a_repeat_that_got_a_different_answer_is_not_reported() {
    // The distinction the verdict cannot make: same call, different
    // result, which is progress rather than a loop.
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "error: cannot borrow `x` as mutable"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "ok, 1 file changed"),
    ];
    assert!(!scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

#[test]
fn synthetic_ids_that_repeat_across_turns_still_compare_the_right_results() {
    // gglib mints tool-call ids itself for dialect models, restarting at
    // zero on every response, so `call_qwen_0` recurs every turn. A join
    // keyed globally by id would resolve both turns to the *last* result
    // and report an identical repeat no matter what came back. The join
    // is positional for exactly this reason.
    let msgs = vec![
        assistant_call_id("call_qwen_0", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("call_qwen_0", "FIRST CONTENT"),
        assistant_call_id("call_qwen_0", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("call_qwen_0", "TOTALLY DIFFERENT CONTENT"),
    ];
    assert!(
        !scan_history(&body(&msgs), &cfg()).identical_result_repeat,
        "results differed; colliding ids must not manufacture a match"
    );
}

#[test]
fn structured_results_that_differ_are_not_reported_as_equal() {
    // Hashing a text-only projection would collapse both of these to the
    // empty string and manufacture a match.
    let msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        json!({ "role": "tool", "tool_call_id": "c1", "content": {"ok": true, "bytes": 1} }),
        assistant_call_id("c2", "write_file", "{}"),
        json!({ "role": "tool", "tool_call_id": "c2", "content": {"ok": false, "bytes": 99999} }),
    ];
    assert!(!scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

#[test]
fn a_malformed_tool_call_id_does_not_disable_the_guard() {
    // The wire types are permissive on purpose: a typed field would fail
    // the whole envelope on a shape quirk and silently switch the guard
    // off for the request.
    let msgs = vec![
        assistant_call("write_file", "{}"),
        json!({ "role": "tool", "tool_call_id": 7, "content": "x" }),
        assistant_call("write_file", "{}"),
        assistant_call("write_file", "{}"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn a_prose_turn_after_a_repeat_clears_the_observation() {
    // Ask, tools, prose answer, follow-up is the ordinary shape of a chat
    // session. If the bit survived the prose turn it would be re-reported on
    // every following request for the same single event.
    let mut msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "same"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "same"),
    ];
    assert!(
        scan_history(&body(&msgs), &cfg()).identical_result_repeat,
        "precondition: the repeat is reported while it is the newest batch"
    );

    for _ in 0..6 {
        msgs.push(assistant_text("here is what I found"));
        msgs.push(json!({ "role": "user", "content": "thanks, next?" }));
        let outcome = scan_history(&body(&msgs), &cfg());
        assert!(
            !outcome.identical_result_repeat,
            "a prose turn ends the observation"
        );
        assert!(!outcome.repeat_not_evaluated);
    }
}

#[test]
fn a_user_interjection_after_a_repeat_clears_the_observation() {
    // A user interrupting mid-turn ends the observation the same way a prose
    // answer does: the request that carried the batch already reported it.
    let mut msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "same"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "same"),
    ];
    assert!(scan_history(&body(&msgs), &cfg()).identical_result_repeat);

    msgs.push(json!({ "role": "user", "content": "stop, do X instead" }));
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(!outcome.identical_result_repeat);
    assert!(!outcome.repeat_not_evaluated);
}

#[test]
fn a_prose_turn_between_two_identical_batches_does_not_hide_the_repeat() {
    // Clearing the bits must not clear the history the comparison is made
    // against — otherwise the fix for the sticky bit becomes an under-report.
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "same"),
        assistant_text("let me think about that"),
        json!({ "role": "user", "content": "go on" }),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "same"),
    ];
    assert!(scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

#[test]
fn a_stagnation_rejection_reports_on_the_message_that_tripped_it() {
    // The bits are computed before the guards run, so a turn carrying both
    // repeated text and a fresh batch does not report the previous batch.
    let repeated_text_with_batch = |id: &str, path: &str| {
        json!({
            "role": "assistant",
            "content": "still working on it",
            "tool_calls": [{
                "id": id, "type": "function",
                "function": { "name": "write_file",
                              "arguments": format!(r#"{{"path":"{path}"}}"#) }
            }]
        })
    };
    let mut msgs = vec![
        assistant_call_id("c0", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c0", "same"),
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "same"),
    ];
    // Now enough repeated *text* turns to trip stagnation, each with a
    // distinct batch of its own.
    for i in 0..8 {
        msgs.push(repeated_text_with_batch(
            &format!("t{i}"),
            &format!("f{i}.rs"),
        ));
        msgs.push(tool_result(&format!("t{i}"), "ok"));
    }
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(matches!(
        outcome.verdict,
        LoopGuardVerdict::StagnationDetected { .. }
    ));
    assert!(
        !outcome.identical_result_repeat,
        "the newest batch was distinct; the stale bit must not survive"
    );
}

#[test]
fn a_two_call_batch_whose_answers_swap_is_not_an_identical_repeat() {
    // Same batch signature, same multiset of results, different pairing.
    // Sorting bare result hashes would have called this identical.
    let msgs = vec![
        json!({
            "role": "assistant", "content": null,
            "tool_calls": [
                { "id": "a1", "type": "function",
                  "function": { "name": "get_errors", "arguments": r#"{"f":"a.rs"}"# } },
                { "id": "b1", "type": "function",
                  "function": { "name": "get_errors", "arguments": r#"{"f":"b.rs"}"# } }
            ]
        }),
        tool_result("a1", "RESULT-ONE"),
        tool_result("b1", "RESULT-TWO"),
        json!({
            "role": "assistant", "content": null,
            "tool_calls": [
                { "id": "a2", "type": "function",
                  "function": { "name": "get_errors", "arguments": r#"{"f":"a.rs"}"# } },
                { "id": "b2", "type": "function",
                  "function": { "name": "get_errors", "arguments": r#"{"f":"b.rs"}"# } }
            ]
        }),
        tool_result("a2", "RESULT-TWO"),
        tool_result("b2", "RESULT-ONE"),
    ];
    assert!(!scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

#[test]
fn object_valued_arguments_do_not_disable_the_guard() {
    // A bare object where the wire format says JSON-encoded string is common
    // client variance. Typed as `String` it failed the whole envelope, and
    // the request was forwarded with no loop protection at all.
    let call = |i: u32| {
        json!({
            "role": "assistant", "content": null,
            "tool_calls": [{
                "id": format!("c{i}"), "type": "function",
                "function": { "name": "write_file", "arguments": { "path": "a.rs" } }
            }]
        })
    };
    let msgs = vec![call(1), call(2), call(3)];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn odd_shapes_anywhere_in_the_history_never_disable_the_guard() {
    // Every field on the wire structs defaults, and none is typed, so no
    // shape quirk can fail the envelope and silently switch the guard off.
    let odd = json!({
        "role": 7,
        "content": { "weird": true },
        "tool_calls": [{ "id": 42, "type": "function",
                         "function": { "name": ["not", "a", "string"], "arguments": 9 } }],
        "tool_call_id": 7
    });
    let msgs = vec![
        odd,
        assistant_call("write_file", "{}"),
        assistant_call("write_file", "{}"),
        assistant_call("write_file", "{}"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn a_repeat_whose_results_are_missing_is_reported_as_not_evaluated() {
    // The state that must not read as "no repeat happened": the batch did
    // repeat, and gglib could not tell whether anything changed.
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(!outcome.identical_result_repeat);
    assert!(outcome.repeat_not_evaluated);
}

#[test]
fn a_first_time_batch_is_neither_a_repeat_nor_unevaluated() {
    let msgs = vec![assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#)];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(!outcome.identical_result_repeat);
    assert!(
        !outcome.repeat_not_evaluated,
        "nothing repeated, so nothing went unevaluated"
    );
}

#[test]
fn a_repeat_with_differing_results_is_evaluated_not_unknown() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "first"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "second"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(!outcome.identical_result_repeat);
    assert!(
        !outcome.repeat_not_evaluated,
        "the question was asked and answered: the results differed"
    );
}

#[test]
fn a_repeat_with_no_result_in_the_history_is_not_reported() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
    ];
    assert!(!scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

#[test]
fn only_the_newest_batch_is_reported_on() {
    // An identical-result repeat early in a long conversation must not
    // still be reported once the model has moved on: the client replays
    // the whole history every turn, and re-reporting it would count one
    // event once per remaining turn.
    let msgs = vec![
        assistant_call_id("c1", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "same"),
        assistant_call_id("c2", "write_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "same"),
        assistant_call_id("c3", "write_file", r#"{"path":"b.rs"}"#),
        tool_result("c3", "moved on"),
    ];
    assert!(!scan_history(&body(&msgs), &cfg()).identical_result_repeat);
}

/// This test was named `reporting_a_repeat_never_changes_a_verdict`, and that
/// invariant is gone: the verdict reads the join now. What survives is the
/// assertion, because the answers here are identical and identical answers are
/// exactly what the guard is for. See ADR 0010.
#[test]
fn a_repeat_with_the_same_answer_still_trips_and_is_reported() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "same"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "same"),
        assistant_call_id("c3", "write_file", "{}"),
        tool_result("c3", "same"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(matches!(
        outcome.verdict,
        LoopGuardVerdict::LoopDetected { .. }
    ));
    assert!(outcome.identical_result_repeat);
}

/// The two paths agree on when a run trips.
///
/// `loop_guard`'s module docs promise parity "by construction — there is one
/// detector implementation, not two". That was an argument, not a check:
/// nothing compared the paths, and the split into `check` and
/// `record_results` is exactly the kind of change that could make one of them
/// forget half the protocol without a single test noticing.
///
/// This drives the same turns through `scan_history` and through a bare
/// detector exercised in the agent's order — check, then record, because on
/// that path the batch has not run at check time. Same detector, same
/// arithmetic, same step.
///
/// What it holds is the *detector's* half on both sides. It does not run the
/// agent loop, so it does not hold `gglib-agent`'s wiring: deleting
/// `guards.record_results` from `run` leaves this test green. That is held by
/// `test_changing_tool_results_do_not_trip_the_loop_guard` instead.
#[test]
fn the_two_paths_agree_on_when_a_run_trips() {
    // (tool, answer) per turn. Chosen so that the recording half is what
    // decides the answer: a different batch breaks the first run, the resumed
    // run is rescued once by a changed answer, and only then does it trip. A
    // path that checks but never records trips one turn earlier, which is what
    // makes this an agreement test rather than a pair of coincidences.
    let turns: &[(&str, &str)] = &[
        ("write_file", "a"),
        ("run_tests", "green"),
        ("write_file", "a"),
        ("write_file", "b"),
        ("write_file", "b"),
        ("write_file", "b"),
    ];

    // Agent path: one detector, check then record, stop at the first refusal.
    let mut det = LoopDetector::default();
    let c = cfg();
    let agent_trips_at = turns.iter().enumerate().find_map(|(i, (tool, answer))| {
        let calls = vec![ToolCall {
            id: format!("c{i}"),
            name: (*tool).to_owned(),
            arguments: json!({}),
        }];
        match det.check(
            &calls,
            c.max_repeated_batch_steps,
            &c.observation_tools,
            c.max_observation_steps,
        ) {
            Err(_) => Some(i),
            Ok(record) => {
                let answers = vec![Some(hash_result_content(&Value::String(
                    (*answer).to_owned(),
                )))];
                det.record_results(record, batch_results_hash(&calls, &answers));
                None
            }
        }
    });

    // Proxy path: the same turns as a replayed history, scanned fresh. The
    // guard sees a prefix of length n and refuses at the same turn the agent
    // did, so the shortest refusing prefix names the step.
    let proxy_trips_at = (1..=turns.len()).find(|n| {
        let mut msgs = Vec::new();
        for (i, (tool, answer)) in turns.iter().take(*n).enumerate() {
            msgs.push(assistant_call_id(&format!("c{i}"), tool, "{}"));
            msgs.push(tool_result(&format!("c{i}"), answer));
        }
        matches!(
            verdict_of(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        )
    });

    assert_eq!(
        agent_trips_at,
        proxy_trips_at.map(|n| n - 1),
        "the paths disagreed about which turn trips"
    );
    assert!(
        agent_trips_at.is_some(),
        "a sequence that never trips proves nothing about agreement"
    );
}

/// Two calls sharing a `tool_call_id`, one answer between them.
///
/// The map join resolved both calls to the single answer present, so a
/// half-answered batch reported as fully joined: `repeat_not_evaluated` read
/// false and the run took a strike from an answer that never existed. The
/// mirror case — one shared id whose answer moves — manufactured a rescue
/// instead. The agent path joins positionally and can do neither, so this was
/// also the one place the two paths could drift.
#[test]
fn duplicate_tool_call_ids_are_unjoinable_rather_than_half_joined() {
    let dup = || {
        json!({
            "role": "assistant",
            "tool_calls": [
                {"id": "c1", "type": "function",
                 "function": {"name": "write_file", "arguments": "{}"}},
                {"id": "c1", "type": "function",
                 "function": {"name": "run_tests", "arguments": "{}"}},
            ]
        })
    };
    let msgs = vec![
        dup(),
        tool_result("c1", "only one answer"),
        dup(),
        tool_result("c1", "only one answer"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(
        outcome.repeat_not_evaluated,
        "an unattributable answer must read as not evaluated: {outcome:?}"
    );
    assert!(!outcome.identical_result_repeat, "{outcome:?}");
    assert!(!outcome.repeat_rescued, "{outcome:?}");
}

/// A rejection must not inherit the previous turn's rescue.
///
/// The other two bits are computed before the guards, so they describe this
/// message even when one rejects it. `repeat_rescued` is only known after
/// `check`, so an early return shipped whatever the last turn set — and since
/// agentic clients replay the whole history, every retry of a 400'd body
/// counted that one rescue again. That inflates precisely the ratio ADR 0010's
/// first kill criterion reads, which is the one that would have the rescue
/// removed. It is also self-contradictory on its face: a verdict saying the
/// guard acted, beside a bit saying it declined to.
///
/// The ceiling path is what makes this observable — the turn before it trips is
/// always a rescue, because reaching the ceiling requires the resets.
#[test]
fn a_rejection_does_not_inherit_the_previous_turns_rescue() {
    let mut msgs = Vec::new();
    for i in 0..15 {
        msgs.push(assistant_call_id(&format!("c{i}"), "write_file", "{}"));
        msgs.push(tool_result(&format!("c{i}"), &format!("{i} files changed")));
    }
    let before = scan_history(&body(&msgs), &cfg());
    assert_eq!(before.verdict, LoopGuardVerdict::Pass);
    assert!(
        before.repeat_rescued,
        "precondition: the 15th turn was a rescue"
    );

    msgs.push(assistant_call_id("c15", "write_file", "{}"));
    msgs.push(tool_result("c15", "brand new"));
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(
        matches!(outcome.verdict, LoopGuardVerdict::LoopDetected { .. }),
        "precondition: the 16th spends the allowance: {outcome:?}"
    );
    assert!(
        !outcome.repeat_rescued,
        "a batch the guard refused was not one it let through: {outcome:?}"
    );
}

/// The stagnation arm of the same defect.
///
/// The round that added `repeat_rescued: false` corrected both early returns
/// but only tested the loop-detected one, so reverting the stagnation arm
/// survived every suite. `a_stagnation_rejection_reports_on_the_message_that_
/// tripped_it` cannot catch it either: its turns use distinct batches, so no
/// rescue ever precedes the trip and the stale value would be `false` anyway.
///
/// This needs a turn that both rescues *and* is followed by a stagnation trip,
/// which means assistant messages carrying text as well as tool calls.
#[test]
fn a_stagnation_rejection_does_not_inherit_the_previous_turns_rescue() {
    let with_text = |id: &str, path: &str| {
        json!({
            "role": "assistant",
            "content": "still working on it",
            "tool_calls": [{
                "id": id,
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": format!(r#"{{"path":"{path}"}}"#),
                },
            }],
        })
    };
    let mut msgs = Vec::new();
    for i in 0..4 {
        msgs.push(with_text(&format!("d{i}"), &format!("f{i}.rs")));
        msgs.push(tool_result(&format!("d{i}"), "ok"));
    }
    // Repeats turn 4's batch with a different answer: a rescue.
    msgs.push(with_text("r1", "f3.rs"));
    msgs.push(tool_result("r1", "different"));
    let mid = scan_history(&body(&msgs), &cfg());
    assert_eq!(mid.verdict, LoopGuardVerdict::Pass, "precondition: {mid:?}");
    assert!(mid.repeat_rescued, "precondition: turn 5 is a rescue");

    // A sixth repeat of the same text trips stagnation.
    msgs.push(with_text("s1", "g.rs"));
    msgs.push(tool_result("s1", "ok"));
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(
        matches!(outcome.verdict, LoopGuardVerdict::StagnationDetected { .. }),
        "precondition: {outcome:?}"
    );
    assert!(
        !outcome.repeat_rescued,
        "a rejected turn must not inherit the previous turn's rescue: {outcome:?}"
    );
}

/// The same, from the answers side: one call, two results claiming to answer it.
///
/// The map kept the last and reported "fully joined" on an arbitrary answer —
/// taking a strike from an answer that never existed, or manufacturing a rescue
/// if that one moved. The calls-side test above never reaches this branch,
/// which is how it survived a mutation round.
#[test]
fn duplicate_answers_for_one_call_are_unjoinable_too() {
    let turn = |a: &str, b: &str| {
        vec![
            assistant_call_id("c1", "write_file", "{}"),
            tool_result("c1", a),
            tool_result("c1", b),
        ]
    };
    let mut msgs = turn("first", "second");
    msgs.extend(turn("first", "third"));
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(
        outcome.repeat_not_evaluated,
        "two answers for one call cannot be attributed: {outcome:?}"
    );
    assert!(!outcome.identical_result_repeat, "{outcome:?}");
    assert!(!outcome.repeat_rescued, "{outcome:?}");
}

/// The rescue has its own reading, because neither bit beside it can show one.
/// `identical_result_repeat` is false here — the answers differ — and
/// `repeat_not_evaluated` is false too, since they joined fine. Without a third
/// counter the turn where the guard declined to act is indistinguishable from a
/// turn where nothing repeated at all.
#[test]
fn a_repeat_the_guard_let_through_is_reported_as_rescued() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "1 file changed"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "2 files changed"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert_eq!(outcome.verdict, LoopGuardVerdict::Pass);
    assert!(outcome.repeat_rescued, "the answer moved: {outcome:?}");
    assert!(!outcome.identical_result_repeat);
    assert!(!outcome.repeat_not_evaluated);
}

/// The rescue bit is cleared by a prose turn, like the two beside it.
///
/// Without the reset it stays set from whatever batch came last and is
/// re-reported on every subsequent request — ask, tools, prose answer,
/// follow-up is the ordinary shape of a chat session, so the inflation is
/// unbounded. An inflated `repeats_rescued` would falsely satisfy ADR 0010's
/// first kill criterion, which is the one that would have the rescue removed.
#[test]
fn a_prose_turn_after_a_rescue_clears_the_observation() {
    let mut msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "one"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "two"),
    ];
    assert!(
        scan_history(&body(&msgs), &cfg()).repeat_rescued,
        "precondition"
    );
    msgs.push(assistant_text("moving on"));
    assert!(!scan_history(&body(&msgs), &cfg()).repeat_rescued);
}

/// And by a user interjection, which takes the other reset arm.
#[test]
fn a_user_interjection_after_a_rescue_clears_the_observation() {
    let mut msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "one"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "two"),
    ];
    assert!(
        scan_history(&body(&msgs), &cfg()).repeat_rescued,
        "precondition"
    );
    msgs.push(json!({"role": "user", "content": "actually, stop"}));
    assert!(!scan_history(&body(&msgs), &cfg()).repeat_rescued);
}

/// And is not reported when the answer stood, or when nothing repeated.
#[test]
fn a_repeat_with_the_same_answer_is_not_reported_as_rescued() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "same"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "same"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert!(!outcome.repeat_rescued);
    assert!(outcome.identical_result_repeat);
}

/// The headline. Three identical `write_file` batches, three different answers.
/// This was a 400 before, on the third turn, at `max_repeated_batch_steps`.
#[test]
fn a_repeat_whose_answer_changed_is_no_longer_a_loop() {
    let msgs = vec![
        assistant_call_id("c1", "write_file", "{}"),
        tool_result("c1", "1 file changed"),
        assistant_call_id("c2", "write_file", "{}"),
        tool_result("c2", "2 files changed"),
        assistant_call_id("c3", "write_file", "{}"),
        tool_result("c3", "3 files changed"),
    ];
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// An answer nobody can read is not evidence of progress. A history with no
/// `role: "tool"` messages at all is unjoinable, and unjoinable never rescues —
/// which is why every loop test written before this change still holds.
#[test]
fn an_unanswered_repeat_still_trips() {
    let msgs = vec![
        assistant_call("write_file", "{}"),
        assistant_call("write_file", "{}"),
        assistant_call("write_file", "{}"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

/// Half an answer is no answer: a batch whose second call went unanswered says
/// nothing about whether work repeated, so it cannot buy a rescue.
#[test]
fn a_partially_answered_repeat_still_trips() {
    let two_calls = |a: &str, b: &str| {
        json!({
            "role": "assistant",
            "tool_calls": [
                {"id": a, "type": "function",
                 "function": {"name": "write_file", "arguments": "{}"}},
                {"id": b, "type": "function",
                 "function": {"name": "run_tests", "arguments": "{}"}},
            ]
        })
    };
    let msgs = vec![
        two_calls("a1", "b1"),
        tool_result("a1", "done"),
        two_calls("a2", "b2"),
        tool_result("a2", "done"),
        two_calls("a3", "b3"),
        tool_result("a3", "done"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

/// A mutating batch cannot be carried forever by an answer that keeps moving.
/// It gets the read-only allowance — fifteen — and the sixteenth trips however
/// new its answer. Without the ceiling this history would pass at any length,
/// and `cargo test`'s `finished in 0.31s` is enough to produce it.
#[test]
fn a_mutating_repeat_with_new_answers_stops_at_the_read_only_allowance() {
    let mut msgs = Vec::new();
    for i in 0..15 {
        msgs.push(assistant_call_id(&format!("c{i}"), "write_file", "{}"));
        msgs.push(tool_result(&format!("c{i}"), &format!("{i} files changed")));
    }
    assert_eq!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::Pass,
        "fifteen occurrences is the allowance, not past it"
    );
    msgs.push(assistant_call_id("c15", "write_file", "{}"));
    msgs.push(tool_result("c15", "brand new"));
    assert!(
        matches!(
            verdict_of(&body(&msgs), &cfg()),
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "the sixteenth must trip"
    );
}

#[test]
fn observation_repeats_are_reported_even_though_they_pass() {
    // The case a verdict can never see: a read_file repeat well under the
    // observation ceiling, whose results were identical. This is the
    // measurement the corrective arm would be built on, if it is built.
    let msgs = vec![
        assistant_call_id("c1", "read_file", r#"{"path":"a.rs"}"#),
        tool_result("c1", "fn main() {}"),
        assistant_call_id("c2", "read_file", r#"{"path":"a.rs"}"#),
        tool_result("c2", "fn main() {}"),
    ];
    let outcome = scan_history(&body(&msgs), &cfg());
    assert_eq!(outcome.verdict, LoopGuardVerdict::Pass);
    assert!(outcome.identical_result_repeat);
}

#[test]
fn shuffled_argument_keys_still_trip() {
    // Same logical arguments, different JSON key order — canonicalized
    // hashing must see them as identical.
    let msgs = vec![
        assistant_call("edit", r#"{"a":1,"b":2}"#),
        assistant_call("edit", r#"{"b":2,"a":1}"#),
        assistant_call("edit", r#"{"a":1,"b":2}"#),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn batch_signature_ignores_call_order() {
    let pair = |first: &str, second: &str| {
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                { "id": "c1", "function": { "name": first, "arguments": "{}" } },
                { "id": "c2", "function": { "name": second, "arguments": "{}" } },
            ]
        })
    };
    let msgs = vec![pair("a", "b"), pair("b", "a"), pair("a", "b")];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn malformed_arguments_hash_as_raw_string() {
    // Not valid JSON — must not 400 the request, and identical malformed
    // batches must still count as repeats.
    let msgs: Vec<Value> = (0..3)
        .map(|_| assistant_call("edit", "{not valid json"))
        .collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn observation_batches_use_elevated_threshold() {
    // 3 identical snapshot calls would trip the standard threshold (2)
    // but pass under the observation threshold (15)…
    let obs: Vec<Value> = (0..3)
        .map(|_| assistant_call("browser_snapshot", "{}"))
        .collect();
    assert_eq!(verdict_of(&body(&obs), &cfg()), LoopGuardVerdict::Pass);

    // …and 16 trips even the elevated threshold.
    let many: Vec<Value> = (0..16)
        .map(|_| assistant_call("browser_snapshot", "{}"))
        .collect();
    assert!(matches!(
        verdict_of(&body(&many), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

#[test]
fn mixed_observation_action_batch_uses_standard_threshold() {
    let mixed = || {
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                { "id": "c1", "function": { "name": "browser_snapshot", "arguments": "{}" } },
                { "id": "c2", "function": { "name": "do_thing", "arguments": "{}" } },
            ]
        })
    };
    let msgs = vec![mixed(), mixed(), mixed()];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

// ── Stagnation detection ─────────────────────────────────────────────────

#[test]
fn five_identical_texts_pass_then_sixth_trips() {
    let five: Vec<Value> = (0..5).map(|_| assistant_text("I am stuck.")).collect();
    assert_eq!(verdict_of(&body(&five), &cfg()), LoopGuardVerdict::Pass);

    let six: Vec<Value> = (0..6).map(|_| assistant_text("I am stuck.")).collect();
    assert_eq!(
        verdict_of(&body(&six), &cfg()),
        LoopGuardVerdict::StagnationDetected {
            count: 6,
            max_steps: 5
        }
    );
}

#[test]
fn oscillation_is_counted_session_wide() {
    // A→B→A→B… trips once either text exceeds the threshold, even though
    // no two consecutive responses match.
    let msgs: Vec<Value> = (0..12)
        .map(|i| assistant_text(if i % 2 == 0 { "plan A" } else { "plan B" }))
        .collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::StagnationDetected { .. }
    ));
}

#[test]
fn content_part_arrays_feed_stagnation() {
    let part_msg = || {
        json!({
            "role": "assistant",
            "content": [
                { "type": "text", "text": "same answer" },
                { "type": "image_url", "image_url": { "url": "ignored" } },
            ]
        })
    };
    let msgs: Vec<Value> = (0..6).map(|_| part_msg()).collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::StagnationDetected { .. }
    ));
}

#[test]
fn null_content_with_tool_calls_feeds_loop_only() {
    // Tool-call-only turns have null content; the empty text must not
    // accumulate stagnation counts (record() skips empty text), so the
    // verdict is the loop detector's, not a stagnation false positive.
    let msgs = vec![
        assistant_call("t", "{}"),
        assistant_call("t", "{}"),
        assistant_call("t", "{}"),
    ];
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

// ── Configuration ────────────────────────────────────────────────────────

#[test]
fn from_settings_gates_on_proxy_loop_detection() {
    let mut settings = Settings::with_defaults();
    assert!(LoopGuardConfig::from_settings(&settings).is_some());

    settings.proxy_loop_detection = Some(true);
    assert!(LoopGuardConfig::from_settings(&settings).is_some());

    settings.proxy_loop_detection = Some(false);
    assert!(LoopGuardConfig::from_settings(&settings).is_none());
}

#[test]
fn from_settings_honours_persisted_stagnation_threshold() {
    let mut settings = Settings::with_defaults();
    settings.max_stagnation_steps = Some(2);
    let cfg = LoopGuardConfig::from_settings(&settings).expect("enabled");

    let three: Vec<Value> = (0..3).map(|_| assistant_text("stuck")).collect();
    assert_eq!(
        verdict_of(&body(&three), &cfg),
        LoopGuardVerdict::StagnationDetected {
            count: 3,
            max_steps: 2
        }
    );
}
