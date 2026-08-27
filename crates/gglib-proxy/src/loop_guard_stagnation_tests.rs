//! Stagnation cases for [`super::scan_history`].
//!
//! A second test module rather than more of `loop_guard_tests.rs`, which is
//! frozen at its current size by the complexity ratchet. The few helpers below
//! are duplicated from it for the same reason.

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

fn assistant_text(text: &str) -> Value {
    json!({ "role": "assistant", "content": text })
}

/// One narrated tool call and its answer.
fn narrated_call(i: usize, path: &str, answer: &str) -> [Value; 2] {
    [
        json!({
            "role": "assistant",
            "content": "Let me look at the file.",
            "tool_calls": [{
                "id": format!("c{i}"),
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": format!(r#"{{"path":"{path}"}}"#)
                }
            }]
        }),
        json!({ "role": "tool", "tool_call_id": format!("c{i}"), "content": answer }),
    ]
}

// ── Narration is not stagnation ───────────────────────────────────────────

/// The case that forced this change.
///
/// Copilot agent mode against a small local model. The model narrates before
/// every call, in the same words, because that is what small models do — and
/// the six calls are six *different* files, which is ordinary work. With
/// `max_stagnation_steps` at its default of 5, the sixth turn was an HTTP 400,
/// and every request after it too, because a replayed history only grows.
#[test]
fn the_same_preamble_before_six_different_reads_is_not_stagnation() {
    let msgs: Vec<Value> = (0..6)
        .flat_map(|i| narrated_call(i, &format!("src/{i}.rs"), "fn main() {}"))
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// The same narration over *identical* reads, which is the case the two
/// detectors could disagree about.
///
/// #923 raised the read-only allowance to 16 precisely so a coding agent could
/// re-read a file without being refused. Counting narration would have undone
/// that at 6 — the observation tier would have been overruled by a guard that
/// cannot see a tool call at all.
#[test]
fn narration_does_not_undercut_the_read_only_allowance() {
    let msgs: Vec<Value> = (0..6)
        .flat_map(|i| narrated_call(i, "src/main.rs", "fn main() {}"))
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// A mutating batch repeated with identical answers is still refused — by the
/// loop detector, which is the guard that can actually see the work. The
/// narration neither causes that nor prevents it.
#[test]
fn a_stuck_mutating_batch_is_still_caught_by_the_other_guard() {
    let msgs: Vec<Value> = (0..3)
        .flat_map(|i| {
            [
                json!({
                    "role": "assistant",
                    "content": "Let me fix it.",
                    "tool_calls": [{
                        "id": format!("w{i}"),
                        "type": "function",
                        "function": { "name": "write_file", "arguments": r#"{"path":"a.rs"}"# }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": format!("w{i}"), "content": "1 changed" }),
            ]
        })
        .collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::LoopDetected { .. }
    ));
}

// ── The window ────────────────────────────────────────────────────────────

/// Six identical prose turns close together are still stagnation. The control
/// for the test below: what changed is the reach of the count, not the count.
#[test]
fn six_identical_prose_turns_close_together_still_stagnate() {
    let msgs: Vec<Value> = (0..6).map(|_| assistant_text("Sure!")).collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::StagnationDetected {
            count: 6,
            max_steps: 5
        }
    ));
}

/// The same six occurrences, spread out. A long chat in which a model says
/// "Sure!" now and then, between fifty turns of real answers, is not stuck —
/// but a tally that never forgot counted those six all the same, and then
/// refused every request for the rest of the conversation.
///
/// Five distinct turns between each pair puts at most four of them inside the
/// 20-turn window the default threshold implies.
#[test]
fn identical_prose_spread_beyond_the_window_is_not_stagnation() {
    let msgs: Vec<Value> = (0..6)
        .flat_map(|i| {
            std::iter::once(assistant_text("Sure!"))
                .chain((0..5).map(move |j| assistant_text(&format!("a distinct answer {i}-{j}"))))
        })
        .collect();
    assert_eq!(verdict_of(&body(&msgs), &cfg()), LoopGuardVerdict::Pass);
}

/// Oscillation survives the window, which is what `WINDOW_FACTOR` is sized
/// for. A → B → A → B trips within 12 turns at the default threshold, well
/// inside the 20 it allows.
///
/// The sibling of `oscillation_is_counted_session_wide`, kept because that
/// test's name now describes a mechanism this detector no longer uses.
#[test]
fn oscillating_prose_still_trips_inside_the_window() {
    let msgs: Vec<Value> = (0..12)
        .map(|i| assistant_text(if i % 2 == 0 { "plan A" } else { "plan B" }))
        .collect();
    assert!(matches!(
        verdict_of(&body(&msgs), &cfg()),
        LoopGuardVerdict::StagnationDetected { .. }
    ));
}
