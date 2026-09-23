//! Tests for the agent path's ledger writer.

use super::*;
use crate::domain::defects::ModelDefectLedger;

/// A quiet decision is still a decision. Without this the trips below would
/// have no denominator, which is the whole defect #1091 is about.
#[test]
fn a_quiet_decision_counts_a_scan_and_no_trip() {
    let ledger = ModelDefectLedger::new();
    ledger.record_agent_guard("a", None);
    ledger.record_agent_guard("a", None);

    let snap = ledger.snapshot()["a"];
    assert_eq!(snap.agent_guard_scanned, 2);
    assert_eq!(snap.agent_guard_trips, 0);
    assert_eq!(snap.agent_guard_loops, 0);
    assert_eq!(snap.agent_guard_stagnations, 0);
}

/// A trip is inside its own denominator, as the proxy's is. A rate computed
/// from a numerator the denominator has never heard of overstates itself, and
/// the two paths would stop being comparable.
#[test]
fn a_trip_is_inside_its_own_denominator() {
    let ledger = ModelDefectLedger::new();
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Stagnation));

    let snap = ledger.snapshot()["a"];
    assert_eq!(
        snap.agent_guard_scanned, 1,
        "the tripping turn is in the denominator too"
    );
    assert_eq!(snap.agent_guard_trips, 1);
}

/// Each detector bumps its own count and the shared total, and never the
/// other's.
#[test]
fn each_detector_is_counted_under_its_own_name() {
    let ledger = ModelDefectLedger::new();
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Loop));
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Loop));
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Stagnation));

    let snap = ledger.snapshot()["a"];
    assert_eq!(snap.agent_guard_loops, 2);
    assert_eq!(snap.agent_guard_stagnations, 1);
}

/// The invariant a reader relies on to avoid double-counting: the total is
/// the sum of the two parts, never their peer.
#[test]
fn the_agent_trips_equal_the_loops_plus_the_stagnations() {
    let ledger = ModelDefectLedger::new();
    for trip in [
        Some(LoopGuardTrip::Loop),
        None,
        Some(LoopGuardTrip::Stagnation),
        Some(LoopGuardTrip::Loop),
        None,
    ] {
        ledger.record_agent_guard("a", trip);
    }

    let snap = ledger.snapshot()["a"];
    assert_eq!(
        snap.agent_guard_scanned, 5,
        "every decision, quiet included"
    );
    assert_eq!(
        snap.agent_guard_trips,
        snap.agent_guard_loops + snap.agent_guard_stagnations
    );
    assert_eq!(snap.agent_guard_trips, 3, "and it is not vacuously zero");
}

/// The two paths share a ledger and must not share a counter. Agent traffic
/// that moved a proxy field would corrupt a reading ADR 0011 rests on.
#[test]
fn agent_traffic_leaves_the_proxy_fields_alone() {
    let ledger = ModelDefectLedger::new();
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Stagnation));
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Loop));

    let snap = ledger.snapshot()["a"];
    assert_eq!(snap.requests, 0, "the agent loop forwards no request");
    assert_eq!(snap.loop_guard_trips, 0);
    assert_eq!(snap.loop_guard_loops, 0);
    assert_eq!(snap.loop_guard_stagnations, 0);
    assert_eq!(snap.agent_guard_trips, 2, "and the agent's own did move");
}

/// The mirror of the above: the proxy's writer leaves the agent fields alone.
#[test]
fn proxy_traffic_leaves_the_agent_fields_alone() {
    let ledger = ModelDefectLedger::new();
    ledger.record_request("a");
    ledger.record_loop_guard_trip("a", LoopGuardTrip::Stagnation);

    let snap = ledger.snapshot()["a"];
    assert_eq!(snap.agent_guard_scanned, 0);
    assert_eq!(snap.agent_guard_trips, 0);
    assert_eq!(snap.agent_guard_stagnations, 0);
    assert_eq!(snap.loop_guard_trips, 1, "and the proxy's own did move");
}

/// Counts are per model, not per ledger.
#[test]
fn the_decisions_are_kept_per_model() {
    let ledger = ModelDefectLedger::new();
    ledger.record_agent_guard("a", Some(LoopGuardTrip::Loop));
    ledger.record_agent_guard("b", None);

    let snap = ledger.snapshot();
    assert_eq!(snap["a"].agent_guard_trips, 1);
    assert_eq!(snap["b"].agent_guard_trips, 0);
    assert_eq!(snap["b"].agent_guard_scanned, 1);
}

/// The port is the ledger's own method, reached through the trait the agent
/// loop holds. A sink wired to something other than the ledger would pass
/// every test above and still record nothing anyone reads.
#[test]
fn the_port_records_into_the_ledger_it_is_called_on() {
    use std::sync::Arc;

    let ledger = Arc::new(ModelDefectLedger::new());
    let sink: Arc<dyn AgentGuardSink> = Arc::clone(&ledger) as Arc<dyn AgentGuardSink>;
    sink.record_decision("a", Some(LoopGuardTrip::Loop));
    sink.record_decision("a", None);

    let snap = ledger.snapshot()["a"];
    assert_eq!(snap.agent_guard_scanned, 2);
    assert_eq!(snap.agent_guard_loops, 1);
}

/// A report stored before these fields existed still reads, with the new
/// counters at zero. `ModelDefectCounts` is read back out of stored benchmark
/// reports, so a field added without this property would make every earlier
/// report unreadable.
#[test]
fn a_report_written_before_these_fields_still_deserialises() {
    let stored = serde_json::json!({
        "requests": 7,
        "loop_guard_trips": 1,
        "loop_guard_loops": 1,
        "loop_guard_stagnations": 0,
        "repairs_attempted": 2,
        "repairs_succeeded": 1,
        "stream_errors": 0,
        "truncated_generations": 0,
        "empty_responses": 0,
        "reasoning_only": 0,
        "dialect_residue": 0,
        "unvalidatable_schemas": 0,
        "normalization_errors": 0,
        "identical_result_repeats": 0,
        "repeats_not_evaluated": 0,
        "repeats_rescued": 0
    });

    let counts: crate::domain::defects::ModelDefectCounts =
        serde_json::from_value(stored).expect("a report from before #1091 still reads");

    assert_eq!(counts.requests, 7, "the fields it did carry are intact");
    assert_eq!(counts.agent_guard_scanned, 0);
    assert_eq!(counts.agent_guard_trips, 0);
    assert_eq!(counts.agent_guard_loops, 0);
    assert_eq!(counts.agent_guard_stagnations, 0);
}
