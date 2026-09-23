//! Tests for [`super::ProxySupervisor::agent_guard_sink`] (#1091).
//!
//! A file of its own rather than more of `supervisor_tests.rs`, which is at
//! 291 lines against the rust-complexity gate's 300 for a file that has never
//! crossed it.

use super::*;
use gglib_core::domain::defects::LoopGuardTrip;

/// The accessor's whole claim: what an agent loop records through the port
/// lands in the ledger the proxy's own runs report into, and therefore in the
/// dashboard that reads it — not in a ledger of its own that nothing reads.
#[test]
fn the_guard_sink_is_the_ledger_the_dashboard_reads() {
    let supervisor = ProxySupervisor::new();

    supervisor
        .agent_guard_sink()
        .record_decision("qwen3", Some(LoopGuardTrip::Loop));

    let counts = supervisor.observers.defects.snapshot();
    let qwen3 = counts.get("qwen3").expect("the model was recorded");
    assert_eq!(qwen3.agent_guard_trips, 1);
    assert_eq!(qwen3.agent_guard_loops, 1);
    assert_eq!(
        qwen3.agent_guard_scanned, 1,
        "a trip is inside its own denominator"
    );
}

/// Two calls hand out the same ledger, so a second agent run does not start a
/// fresh population — the property the field's own documentation claims when
/// it says the counts outlive a proxy restart.
#[test]
fn every_caller_gets_the_same_ledger() {
    let supervisor = ProxySupervisor::new();

    supervisor.agent_guard_sink().record_decision("qwen3", None);
    supervisor
        .agent_guard_sink()
        .record_decision("qwen3", Some(LoopGuardTrip::Stagnation));

    let counts = supervisor.observers.defects.snapshot();
    let qwen3 = counts.get("qwen3").expect("the model was recorded");
    assert_eq!(qwen3.agent_guard_scanned, 2);
    assert_eq!(qwen3.agent_guard_stagnations, 1);
}

/// The agent path's counters are its own: nothing recorded here reaches the
/// proxy's, which describe a different decision taken at a different moment.
#[test]
fn agent_traffic_leaves_the_proxy_fields_alone() {
    let supervisor = ProxySupervisor::new();

    supervisor
        .agent_guard_sink()
        .record_decision("qwen3", Some(LoopGuardTrip::Loop));

    let counts = supervisor.observers.defects.snapshot();
    let qwen3 = counts.get("qwen3").expect("the model was recorded");
    assert_eq!(qwen3.loop_guard_trips, 0);
    assert_eq!(qwen3.loop_guard_loops, 0);
    assert_eq!(
        qwen3.requests, 0,
        "`requests` is the proxy's denominator; an agent turn is not a proxy request"
    );
}
