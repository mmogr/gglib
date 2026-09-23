//! Tests for the agent path's rows in the per-model signals section (#1091).
//!
//! Their own file rather than more of `render_defects_tests.rs`, which the
//! file-size budget has no room left in.
//!
//! What they are mostly about is the two ways this section could lie about a
//! model reached only through GUI chat: filtering it out for having a clean
//! proxy record, and printing its missing proxy denominator as a zero.

use super::*;

/// Like the sibling module's helper. A model that has served proxy traffic,
/// so a test can turn that off deliberately and mean it.
fn counts(build: impl FnOnce(&mut ModelDefectCounts)) -> ModelDefectCounts {
    let mut c = ModelDefectCounts {
        requests: 100,
        ..Default::default()
    };
    build(&mut c);
    c
}

/// A model reached only through GUI chat has forwarded nothing, so its
/// agent-path trips are the only thing it has to report. Before #1091 they
/// were not counted at all; a dashboard that then filtered the model out for
/// having a clean proxy record would hide the one path that failed.
#[test]
fn a_model_whose_only_signal_is_an_agent_trip_still_earns_a_line() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.requests = 0;
            c.agent_guard_scanned = 12;
            c.agent_guard_trips = 1;
            c.agent_guard_stagnations = 1;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("qwen"), "{rendered}");
    assert!(
        rendered.contains("agent-path guard trips"),
        "the trip row is the whole reason the model is listed: {rendered}"
    );
    assert!(
        rendered.contains("stagnation detector"),
        "and which detector raised it: {rendered}"
    );
}

/// The trips carry their own denominator, because the header's `requests` is
/// not it: one client conversation is many agent turns, and a proxy request
/// is neither.
#[test]
fn agent_trips_print_against_the_decisions_they_were_taken_over() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.agent_guard_scanned = 40;
            c.agent_guard_trips = 3;
            c.agent_guard_loops = 3;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(
        rendered.contains("3 of 40 decision(s)"),
        "a trip count with its denominator somewhere else is what #1091 is about: {rendered}"
    );
}

/// "0 request(s)" on a model only ever reached through GUI chat reads as
/// "nothing happened", when what happened was on the other path.
#[test]
fn a_model_with_no_proxy_traffic_says_so_rather_than_printing_zero() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.requests = 0;
            c.agent_guard_scanned = 9;
            c.agent_guard_trips = 1;
            c.agent_guard_loops = 1;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("no proxy requests"), "{rendered}");
    assert!(
        !rendered.contains("0 request(s)"),
        "the misleading zero must not also print: {rendered}"
    );
}

/// Turns the guard looked at and passed are a denominator, not a defect. A
/// model with nothing but those has nothing to report, exactly as one with
/// nothing but `requests` has not — but the turns still belong in the
/// denominator the "none" is claimed over.
#[test]
fn agent_turns_that_never_tripped_do_not_list_the_model() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.agent_guard_scanned = 30))]);

    let rendered = render_defects_section(&per_model);
    assert!(
        !rendered.contains("agent-path guard trips"),
        "nothing tripped: {rendered}"
    );
    assert!(
        rendered.contains("none across 100 request(s) and 30 agent turn(s)"),
        "but the turns are the denominator the 'none' is claimed over: {rendered}"
    );
}

/// A proxy that predates #1091 sends none of the four, and the section must
/// render exactly as it did before.
#[test]
fn an_older_proxys_frame_prints_no_agent_row() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.stream_errors = 2))]);

    let rendered = render_defects_section(&per_model);
    assert!(
        !rendered.contains("agent-path"),
        "no agent traffic, no agent row: {rendered}"
    );
    assert!(rendered.contains("100 request(s)"), "{rendered}");
}
