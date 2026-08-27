//! What a user turn resets, on both detectors.
//!
//! A third test module because `loop_guard_tests.rs` is frozen at its current
//! size by the complexity ratchet. Helpers are duplicated for the same reason.

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

fn verdict_of(body: &[u8], cfg: &LoopGuardConfig) -> LoopGuardVerdict {
    scan_history(body, cfg).verdict
}

fn user(text: &str) -> Value {
    json!({ "role": "user", "content": text })
}

/// One `make` run and the identical error it produced.
fn build_attempt(i: usize) -> [Value; 3] {
    [
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": format!("b{i}"),
                "type": "function",
                "function": { "name": "run_in_terminal", "arguments": r#"{"cmd":"make"}"# }
            }]
        }),
        json!({ "role": "tool", "tool_call_id": format!("b{i}"), "content": "error: 3 warnings emitted" }),
        json!({ "role": "assistant", "content": "the build failed" }),
    ]
}

// ── The loop detector ─────────────────────────────────────────────────────

/// The scenario the audit named, in full.
///
/// A person asks for a build, it fails, and they ask twice more. The batch is
/// identical each time and so is its answer, so nothing rescues it — and
/// `run_in_terminal` is not an observation tool, so it is held to
/// `max_repeated_batch_steps`, which is 2. The third request was an HTTP 400.
///
/// The same conversation through `gglib-agent` never tripped, because a user
/// message starts a fresh `AgentLoop::run` there.
#[test]
fn three_user_requests_for_the_same_failing_build_are_not_a_loop() {
    let mut msgs = vec![user("run the build")];
    msgs.extend(build_attempt(0));
    msgs.push(user("try again please"));
    msgs.extend(build_attempt(1));
    msgs.push(user("try again please"));
    msgs.extend(build_attempt(2));
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// The control: the same three attempts with nobody asking for them is a model
/// spinning on its own, and is still refused. What changed is who is driving,
/// not how many repeats are allowed.
#[test]
fn the_same_three_attempts_unprompted_are_still_a_loop() {
    let msgs: Vec<Value> = (0..3).flat_map(build_attempt).collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

/// A user turn resets the read-only allowance too, not only the strike count.
///
/// `Run::total` is what bounds a batch carried by changing answers, and a fresh
/// `Guards` on the agent path clears it along with everything else. Keeping it
/// across a user turn would have been a second, quieter divergence.
#[test]
fn a_user_turn_resets_the_read_only_allowance_as_well() {
    let attempt = |i: usize| {
        [
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": format!("w{i}"),
                    "type": "function",
                    "function": { "name": "write_file", "arguments": r#"{"path":"a.rs"}"# }
                }]
            }),
            // A moving answer, so each repeat is rescued and only `total` grows.
            json!({ "role": "tool", "tool_call_id": format!("w{i}"), "content": format!("{i} changed") }),
        ]
    };
    let mut msgs: Vec<Value> = (0..15).flat_map(attempt).collect();
    assert_eq!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::Pass,
        "precondition: fifteen rescued repeats are inside the allowance"
    );
    msgs.push(user("carry on"));
    msgs.extend((15..29).flat_map(attempt));
    assert_eq!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::Pass,
        "the interjection must restore the allowance, not merely the strikes"
    );
}

// ── The stagnation detector ───────────────────────────────────────────────

/// Repeats either side of a user turn are two separate spans, not one run of
/// six. Without the reset the model would be refused for saying the same thing
/// three times, being redirected, and saying a different same thing three more.
#[test]
fn prose_repeats_on_either_side_of_a_user_turn_do_not_accumulate() {
    let say = |t: &str| json!({ "role": "assistant", "content": t });
    let mut msgs: Vec<Value> = (0..3).map(|_| say("I cannot proceed.")).collect();
    msgs.push(user("try a different approach"));
    msgs.extend((0..3).map(|_| say("I cannot proceed.")));
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// The control, and the limit of this change. Six in a row with nobody
/// interrupting still stagnates — and because `scan_history` returns on the
/// first trip it finds, it keeps stagnating on every later request, whatever
/// the user says afterwards.
///
/// Recovering *that* means not returning early, which is a different change:
/// a verdict about the end of the transcript rather than its worst moment
/// could be cleared by any client that appends a trailing user message, which
/// is most of them. It needs its own argument and is not made here.
#[test]
fn six_adjacent_prose_repeats_still_stagnate_whatever_follows() {
    let mut msgs: Vec<Value> = (0..6)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed." }))
        .collect();
    msgs.push(user("try a different approach"));
    msgs.push(json!({ "role": "assistant", "content": "Here is another way." }));
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::StagnationDetected { .. }
    ));
}

/// A system message is not a person taking the wheel, but it shares the arm —
/// and it sits at the head of the conversation, where there is no run to break.
/// Pinned so that treating the two alike stays a decision rather than a
/// discovery.
#[test]
fn a_leading_system_message_changes_nothing() {
    let mut msgs = vec![json!({ "role": "system", "content": "be helpful" })];
    msgs.extend((0..3).flat_map(build_attempt));
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}
