//! Tests for [`super`]: the day arithmetic, and what an event keeps of the
//! text it was built from.

use super::*;

#[test]
fn a_day_is_counted_in_whole_utc_days_from_the_epoch() {
    assert_eq!(epoch_day(0), 0);
    assert_eq!(epoch_day(86_399), 0);
    assert_eq!(epoch_day(86_400), 1);
    // 2026-09-18T00:00:00Z.
    assert_eq!(epoch_day(1_789_689_600), 20_714);
}

#[test]
fn a_window_ends_today_and_is_clamped_to_the_retention() {
    let now = 86_400 * 100 + 5;
    assert_eq!(first_day_of_window(now, 1), 100);
    assert_eq!(first_day_of_window(now, 7), 94);
    assert_eq!(first_day_of_window(now, 0), 100, "zero days is today alone");
    assert_eq!(
        first_day_of_window(now, 10_000),
        11,
        "no wider than the 90 days the log keeps"
    );
}

#[test]
fn a_model_name_is_cut_at_the_limit_on_a_character_boundary() {
    let long = "é".repeat(300);
    let kept = bounded_model_name(&long);
    assert_eq!(kept.chars().count(), 256);
    assert_eq!(bounded_model_name("qwen3"), "qwen3");
}

#[test]
fn a_signature_is_kept_as_a_known_hash_and_never_as_itself() {
    let event = LoopGuardTripEvent::new(7, "m", LoopGuardTrip::Loop, LoopGuardMode::Note)
        .with_signature("write_file:1f2e3d4c5b6a7980");
    // The literal pins the hash function itself: a switch to a hasher whose
    // output is not stable across releases fails here, not in a year's data.
    assert_eq!(event.signature_hash(), Some("f219abe8cefbf0db"));
}

#[test]
fn no_text_the_event_was_built_from_survives_in_it() {
    let event = LoopGuardTripEvent::new(7, "model", LoopGuardTrip::Loop, LoopGuardMode::Refuse)
        .with_signature("SENTINELtool:00000000deadbeef")
        .with_session("sentinel-session");

    assert_eq!(event.session_hash(), Some("dbe38cf4ae365a2b"));
    let everything = format!("{event:?}").to_lowercase();
    assert!(
        !everything.contains("sentinel"),
        "a tool name or a session id reached the event: {everything}"
    );
}

#[test]
fn repeats_are_carried_as_given() {
    let event = LoopGuardTripEvent::new(1, "model", LoopGuardTrip::Stagnation, LoopGuardMode::Note)
        .with_repeats(6, 5);
    assert_eq!(event.repeat_count(), Some(6));
    assert_eq!(event.threshold(), Some(5));
    assert_eq!(event.signature_hash(), None);
}
