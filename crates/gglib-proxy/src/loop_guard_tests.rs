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

#[test]
fn reporting_a_repeat_never_changes_a_verdict() {
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
