//! What a verdict under `note` is about: the newest turn each detector judged,
//! not the worst moment in the transcript (ADR 0021). `refuse` still acts on
//! the first trip anywhere, and a control here pins that.
//!
//! A module of its own because `loop_guard_tests.rs` is frozen at its size by
//! the complexity ratchet. Helpers are duplicated for the same reason.

use super::*;
use serde_json::{Value, json};

fn cfg_in(mode: LoopGuardMode) -> LoopGuardConfig {
    let settings = Settings {
        loop_guard_mode: Some(mode),
        ..Settings::with_defaults()
    };
    LoopGuardConfig::from_settings(&settings).expect("a mode that scans")
}

fn note() -> LoopGuardConfig {
    cfg_in(LoopGuardMode::Note)
}

fn body(messages: &[Value]) -> Vec<u8> {
    json!({ "model": "m", "messages": messages })
        .to_string()
        .into_bytes()
}

fn user(text: &str) -> Value {
    json!({ "role": "user", "content": text })
}

fn say(text: &str) -> Value {
    json!({ "role": "assistant", "content": text })
}

/// `n` turns writing `path`, each answered the same way. A mutating tool, so
/// it is held to `max_repeated_batch_steps` (2): the third in a row trips.
fn writes(path: &str, n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|i| {
            let id = format!("{path}-{i}");
            [
                json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": json!({ "path": path }).to_string()
                        }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": id, "content": "1 file changed" }),
            ]
        })
        .collect()
}

fn is_loop(outcome: &ScanOutcome) -> bool {
    matches!(outcome.verdict, LoopGuardVerdict::LoopDetected { .. })
}

#[test]
fn a_tripped_run_followed_by_two_other_batches_passes() {
    let mut msgs = writes("a.rs", 3);
    assert!(
        is_loop(&scan_history(&body(&msgs), &note())),
        "precondition: three in a row trip"
    );
    msgs.extend(writes("b.rs", 1));
    msgs.extend(writes("c.rs", 1));
    assert_eq!(
        scan_history(&body(&msgs), &note()).verdict,
        LoopGuardVerdict::Pass
    );
}

#[test]
fn a_tripped_run_then_a_user_turn_then_another_batch_passes() {
    let mut msgs = writes("a.rs", 3);
    msgs.push(user("try something else"));
    msgs.extend(writes("b.rs", 1));
    assert_eq!(
        scan_history(&body(&msgs), &note()).verdict,
        LoopGuardVerdict::Pass
    );
}

/// The same answer, in prose: a person spoke and the model answered them.
#[test]
fn a_tripped_run_then_a_user_turn_then_a_prose_reply_passes() {
    let mut msgs = writes("a.rs", 3);
    msgs.push(user("what are you doing?"));
    msgs.push(say("I was rewriting a.rs."));
    assert_eq!(
        scan_history(&body(&msgs), &note()).verdict,
        LoopGuardVerdict::Pass
    );
}

/// A user turn with nothing after it is not the model changing anything. A
/// client that appends a user message to every request must not clear a trip
/// by doing so.
#[test]
fn a_trailing_user_turn_alone_does_not_clear_a_trip() {
    let mut msgs = writes("a.rs", 3);
    msgs.push(user("continue"));
    let outcome = scan_history(&body(&msgs), &note());
    assert!(is_loop(&outcome), "{outcome:?}");
    assert!(!outcome.tripped_before(), "this is the run's first trip");
}

#[test]
fn six_repeats_at_the_end_still_trip() {
    let outcome = scan_history(&body(&writes("a.rs", 6)), &note());
    assert!(is_loop(&outcome), "{outcome:?}");
    assert!(outcome.tripped_before(), "the fifth tripped too");
}

/// The bit part B reads: the first trip of a run is noted, a second is not
/// its first.
#[test]
fn only_a_runs_second_trip_says_it_tripped_before() {
    let first = scan_history(&body(&writes("a.rs", 3)), &note());
    assert!(is_loop(&first) && !first.tripped_before(), "{first:?}");
    let second = scan_history(&body(&writes("a.rs", 4)), &note());
    assert!(is_loop(&second) && second.tripped_before(), "{second:?}");
}

/// A run that ended and came back is a new run: noted again, not ended.
#[test]
fn a_run_that_comes_back_after_another_batch_starts_its_streak_again() {
    let mut msgs = writes("a.rs", 3);
    msgs.extend(writes("b.rs", 1));
    msgs.extend(writes("a.rs", 3));
    let outcome = scan_history(&body(&msgs), &note());
    assert!(is_loop(&outcome), "{outcome:?}");
    assert!(!outcome.tripped_before(), "{outcome:?}");
}

#[test]
fn a_prose_run_says_it_tripped_before_from_its_second_trip() {
    let prose = |n: usize| -> Vec<Value> { (0..n).map(|_| say("I cannot proceed.")).collect() };
    let first = scan_history(&body(&prose(6)), &note());
    assert!(
        matches!(first.verdict, LoopGuardVerdict::StagnationDetected { .. }),
        "{first:?}"
    );
    assert!(!first.tripped_before());
    assert!(scan_history(&body(&prose(7)), &note()).tripped_before());
}

/// Each detector is blind to the other's turns, so both can stand at the end.
/// The later trip is reported.
#[test]
fn the_later_of_two_standing_trips_is_the_one_reported() {
    let prose: Vec<Value> = (0..6).map(|_| say("I cannot proceed.")).collect();

    let mut prose_then_batches = prose.clone();
    prose_then_batches.extend(writes("a.rs", 3));
    assert!(is_loop(&scan_history(&body(&prose_then_batches), &note())));

    let mut batches_then_prose = writes("a.rs", 3);
    batches_then_prose.extend(prose);
    assert!(matches!(
        scan_history(&body(&batches_then_prose), &note()).verdict,
        LoopGuardVerdict::StagnationDetected { .. }
    ));
}

/// The control: `refuse` acts on the first trip anywhere, as before.
#[test]
fn under_refuse_the_first_trip_still_stands_whatever_follows() {
    let mut msgs = writes("a.rs", 3);
    msgs.extend(writes("b.rs", 1));
    msgs.extend(writes("c.rs", 1));
    let outcome = scan_history(&body(&msgs), &cfg_in(LoopGuardMode::Refuse));
    assert!(is_loop(&outcome), "{outcome:?}");
    assert!(
        !outcome.tripped_before(),
        "refuse never reaches a second trip"
    );
}

/// A person who speaks after a run's second trip is not answered by the
/// ending: the trip still stands, but as a first one, so their turn is noted
/// and the model gets to answer them.
#[test]
fn a_person_speaking_after_a_second_trip_is_noted_not_ended() {
    let mut msgs = writes("a.rs", 4);
    assert!(scan_history(&body(&msgs), &note()).tripped_before());
    msgs.push(say("[gglib loop guard] stopped this run."));
    msgs.push(user("try a different file"));
    let outcome = scan_history(&body(&msgs), &note());
    assert!(is_loop(&outcome), "{outcome:?}");
    assert!(!outcome.tripped_before(), "{outcome:?}");
}
