//! What the agent loop reports about its own guard (#1091).
//!
//! Before this, the loop ran `LoopDetector` and `StagnationDetector` and told
//! nobody what they decided, so `loop_guard_trips` described the proxy alone.
//! These tests are about the decisions reaching a sink, and about the two ways
//! that count could be wrong: a turn the guard ran on and did not report, and
//! a turn it reported without having run.
//!
//! The recorder and the loops driven at it are in `common::guard_recorder`.

mod common;

use std::sync::Arc;

use common::guard_recorder::{
    MODEL, Recorder, one_call_then_answer, repeating_llm, reporter, run_to_end, unchanging_executor,
};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::MockToolExecutorPort;
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::AgentConfig;
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::ports::{AgentError, AgentLoopPort};

/// Both detectors on, at the thresholds the guard normally runs with.
fn guarded() -> AgentConfig {
    common::for_test(|c| {
        c.max_iterations = 10;
        c.max_repeated_batch_steps = Some(2);
        c.max_stagnation_steps = Some(5);
    })
}

/// The same batch, back to back, answered the same way each time.
fn looping_agent(recorder: &Arc<Recorder>) -> Arc<dyn AgentLoopPort> {
    AgentLoop::build_observed(
        repeating_llm(),
        Arc::new(unchanging_executor()),
        None,
        Some(reporter(recorder)),
    )
}

#[tokio::test]
async fn a_loop_trip_is_reported_and_names_the_loop_detector() {
    let recorder = Arc::new(Recorder::default());
    let result = run_to_end(&looping_agent(&recorder), guarded()).await;

    assert!(
        matches!(result, Err(AgentError::LoopDetected { .. })),
        "the run must still end the way it did before: {result:?}"
    );
    assert_eq!(
        recorder.trips(),
        vec![LoopGuardTrip::Loop],
        "one trip, raised by the loop detector"
    );
}

/// `max_stagnation_steps = 0` is the only threshold that can reach this
/// detector on the agent path.
///
/// `StagnationDetector` ignores any turn that made tool calls, and a turn that
/// made none is the final answer in an agent run — so a run records at most
/// one turn and cannot reach a threshold above zero. Zero fires on the first
/// occurrence. ADR 0011 calls the detector "nearly inert" here for the same
/// reason, which is why the counter it feeds is documented as a warning
/// rather than as evidence.
#[tokio::test]
async fn a_stagnation_trip_names_the_stagnation_detector() {
    let recorder = Arc::new(Recorder::default());
    let agent = AgentLoop::build_observed(
        Arc::new(MockLlmPort::new().push(MockLlmResponse::text("the same prose"))),
        Arc::new(MockToolExecutorPort::new()),
        None,
        Some(reporter(&recorder)),
    );

    let result = run_to_end(
        &agent,
        common::for_test(|c| {
            c.max_stagnation_steps = Some(0);
            c.max_repeated_batch_steps = Some(2);
        }),
    )
    .await;

    assert!(
        matches!(result, Err(AgentError::StagnationDetected { .. })),
        "{result:?}"
    );
    assert_eq!(recorder.trips(), vec![LoopGuardTrip::Stagnation]);
}

/// A turn that called a tool and did not trip is still a turn the guard ran
/// on. Without it the trips above would have no denominator.
#[tokio::test]
async fn a_quiet_tool_call_turn_is_reported_with_no_trip() {
    let recorder = Arc::new(Recorder::default());
    let agent = AgentLoop::build_observed(
        one_call_then_answer(),
        Arc::new(unchanging_executor()),
        None,
        Some(reporter(&recorder)),
    );

    let result = run_to_end(&agent, guarded()).await;

    assert!(result.is_ok(), "{result:?}");
    let calls = recorder.calls();
    assert_eq!(
        calls.len(),
        2,
        "the tool-call turn and the final answer are both decisions: {calls:?}"
    );
    assert!(
        calls.iter().all(|(_, trip)| trip.is_none()),
        "nothing tripped: {calls:?}"
    );
}

/// The other quiet exit. A text-only first turn is the final answer, and the
/// guard still ran on it.
#[tokio::test]
async fn a_quiet_text_only_turn_is_reported_with_no_trip() {
    let recorder = Arc::new(Recorder::default());
    let agent = AgentLoop::build_observed(
        Arc::new(MockLlmPort::new().push(MockLlmResponse::text("here you go"))),
        Arc::new(MockToolExecutorPort::new()),
        None,
        Some(reporter(&recorder)),
    );

    let result = run_to_end(&agent, guarded()).await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(recorder.calls().len(), 1);
    assert_eq!(recorder.trips(), Vec::new());
}

/// No guard, no scan. The proxy records nothing when its guard is off, and a
/// denominator counting unguarded turns would not be comparable with it.
#[tokio::test]
async fn a_run_with_both_guards_disabled_reports_nothing() {
    let recorder = Arc::new(Recorder::default());
    let agent = AgentLoop::build_observed(
        one_call_then_answer(),
        Arc::new(unchanging_executor()),
        None,
        Some(reporter(&recorder)),
    );

    let result = run_to_end(
        &agent,
        common::for_test(|c| {
            c.max_repeated_batch_steps = None;
            c.max_stagnation_steps = None;
        }),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        recorder.calls(),
        Vec::new(),
        "no detector was enabled, so no turn belongs in any denominator"
    );
}

/// Reporting is an observation, not a step of the loop. A build with no
/// reporter behaves exactly as `AgentLoop::build` always did.
#[tokio::test]
async fn no_reporter_is_a_no_op_and_the_trip_still_ends_the_run() {
    let agent = AgentLoop::build(repeating_llm(), Arc::new(unchanging_executor()), None);

    let result = run_to_end(&agent, guarded()).await;

    assert!(
        matches!(result, Err(AgentError::LoopDetected { .. })),
        "{result:?}"
    );
}

/// The counts are per model, and the name comes from whoever composed the
/// loop. A sink told the wrong name files real traffic under a model that was
/// never run.
#[tokio::test]
async fn every_decision_is_recorded_under_the_reporters_model() {
    let recorder = Arc::new(Recorder::default());
    let _ = run_to_end(&looping_agent(&recorder), guarded()).await;

    let calls = recorder.calls();
    assert!(!calls.is_empty(), "the run has to have decided something");
    assert!(
        calls.iter().all(|(model, _)| model == MODEL),
        "every decision under the composed model's name: {calls:?}"
    );
}

/// The property the whole field set rests on: the tripping turn is in the
/// denominator too. A trip counted outside the turns it was taken over would
/// overstate every rate computed from these numbers.
#[tokio::test]
async fn a_trip_is_reported_inside_its_own_denominator() {
    let recorder = Arc::new(Recorder::default());
    let _ = run_to_end(&looping_agent(&recorder), guarded()).await;

    let calls = recorder.calls();
    let trips = calls.iter().filter(|(_, t)| t.is_some()).count();
    assert_eq!(trips, 1, "one trip: {calls:?}");
    assert_eq!(
        calls.len(),
        3,
        "over three decisions, the tripping one included: {calls:?}"
    );
    assert!(
        calls.last().is_some_and(|(_, t)| t.is_some()),
        "and it is the last one, which ended the run: {calls:?}"
    );
}
