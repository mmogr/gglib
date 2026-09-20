//! Tests for the A/B eval in `agentic.rs`.
//!
//! Split from `agentic.rs` so the module stays inside the complexity
//! ratchet's budget — the repo's `*_tests.rs` sibling pattern.

use super::*;
use gglib_core::domain::benchmark::tune::result::GeneratedOutput;
use gglib_core::domain::benchmark::tune::task::{ExpectedCall, TaskCategory};

fn call_task(expected: ExpectedOutcome) -> TuneTask {
    TuneTask {
        id: "t".into(),
        category: TaskCategory::SingleCall,
        system_prompt: None,
        history: None,
        user_prompt: "do it".into(),
        tools: vec![],
        expected,
    }
}

/// Build a config from JSON so every test also exercises the serde
/// defaults, which are what a daemon request actually arrives carrying.
fn config(extra: &str) -> AgenticEvalConfig {
    let json = format!(r#"{{"model_id": 1, "task_suite": {{"source": "default"}}{extra}}}"#);
    serde_json::from_str(&json).expect("deserializes")
}

fn seeds_of(plans: &[ArmPlan], arm: EvalArm) -> Vec<Option<u32>> {
    plans
        .iter()
        .find(|p| p.arm == arm)
        .map(|p| p.seeds.clone())
        .unwrap_or_default()
}

/// The two real arms are compared with each other, so any asymmetry in
/// their seeds would land in the delta rather than in the pipeline.
#[test]
fn the_two_real_arms_share_a_seed_set() {
    let plans = plan_arms(&config(r#", "seeds": [1, 2, 3]"#));

    assert_eq!(
        seeds_of(&plans, EvalArm::Raw),
        seeds_of(&plans, EvalArm::Gglib)
    );
    assert_eq!(seeds_of(&plans, EvalArm::Raw).len(), 3);
}

/// **The whole design of the A/A arm.** Sharing seeds with the raw arm
/// would measure decode determinism instead of the seed-draw variance that
/// actually bounds the primary comparison.
#[test]
fn the_replicate_arm_runs_different_seeds_of_the_same_size() {
    let plans = plan_arms(&config(r#", "seeds": [1, 2, 3]"#));

    let raw = seeds_of(&plans, EvalArm::Raw);
    let replicate = seeds_of(&plans, EvalArm::RawReplicate);
    assert_eq!(
        replicate.len(),
        raw.len(),
        "same sample size, or the two \
            composites are not comparable"
    );
    for seed in &replicate {
        assert!(!raw.contains(seed), "{seed:?} was reused");
    }
}

/// The expensive arm stops paying for precision nothing reads: one seed,
/// not the run's five.
#[test]
fn the_control_repeats_fewer_seeds_than_the_real_arms() {
    let plans = plan_arms(&config(r#", "seeds": [1, 2, 3, 4, 5]"#));

    assert_eq!(seeds_of(&plans, EvalArm::Control), vec![Some(1)]);
    assert_eq!(seeds_of(&plans, EvalArm::Raw).len(), 5);
}

/// Zero would plan an arm with no runs, whose empty scores would then be
/// compared against as though they had been measured.
#[test]
fn a_control_seed_count_of_zero_still_runs_once() {
    let plans = plan_arms(&config(r#", "seeds": [1, 2], "control_seeds": 0"#));

    assert_eq!(seeds_of(&plans, EvalArm::Control).len(), 1);
}

/// And asking for more seeds than the run has cannot invent them.
#[test]
fn a_control_seed_count_above_the_run_is_clamped_down() {
    let plans = plan_arms(&config(r#", "seeds": [1, 2], "control_seeds": 9"#));

    assert_eq!(seeds_of(&plans, EvalArm::Control).len(), 2);
}

/// An unseeded run is the fast smoke test, and the A/A arm still means
/// something there: nothing was pinned, so repeating the request measures
/// full decode variance.
#[test]
fn an_unseeded_run_still_plans_every_arm_once() {
    let plans = plan_arms(&config(r#", "seeds": []"#));

    for arm in [
        EvalArm::Raw,
        EvalArm::Gglib,
        EvalArm::RawReplicate,
        EvalArm::Control,
    ] {
        assert_eq!(seeds_of(&plans, arm), vec![None], "{arm}");
    }
}

/// Opting out of either calibration arm removes it and nothing else.
#[test]
fn the_calibration_arms_are_individually_optional() {
    let no_control = plan_arms(&config(r#", "include_control": false"#));
    let no_replicate = plan_arms(&config(r#", "replicate_raw": false"#));

    assert!(!no_control.iter().any(|p| p.arm == EvalArm::Control));
    assert!(no_control.iter().any(|p| p.arm == EvalArm::RawReplicate));
    assert!(!no_replicate.iter().any(|p| p.arm == EvalArm::RawReplicate));
    assert!(no_replicate.iter().any(|p| p.arm == EvalArm::Control));
}

/// The control is the most expensive arm by an order of magnitude, so an
/// interrupted run should already have both real arms and the cheap A/A
/// one before it starts.
#[test]
fn the_control_is_planned_last() {
    let plans = plan_arms(&config(""));

    assert_eq!(plans.last().map(|p| p.arm), Some(EvalArm::Control));
}

/// Results are taken by arm rather than popped in push order, so an arm
/// that did not run yields nothing instead of another arm's scores.
#[test]
fn taking_an_arm_that_did_not_run_yields_nothing() {
    let mut results = vec![(EvalArm::Raw, vec![vec![]]), (EvalArm::Gglib, vec![vec![]])];

    assert!(take_arm(&mut results, EvalArm::Control).is_none());
    assert!(take_arm(&mut results, EvalArm::Gglib).is_some());
    assert!(
        take_arm(&mut results, EvalArm::Gglib).is_none(),
        "and it is removed, not cloned"
    );
    assert!(take_arm(&mut results, EvalArm::Raw).is_some());
}

pub(super) fn run(passed: bool, unmeasured: Option<&str>) -> TuneTaskResult {
    TuneTaskResult {
        task_id: "t".to_owned(),
        category: TaskCategory::SingleCall,
        passed,
        tool_match_score: if passed { 1.0 } else { 0.0 },
        loop_detected: false,
        stagnation_detected: false,
        iterations: 1,
        latency_ms: 10,
        completion_tokens: None,
        time_to_first_tool_call_ms: None,
        detail: None,
        unmeasured: unmeasured.map(ToOwned::to_owned),
        transport_retries: 0,
        generated: GeneratedOutput::default(),
    }
}

/// **The failure this whole check exists for.** 45 runs against a dead
/// upstream produce a composite that is arithmetically correct and
/// completely empty, and it must abort rather than be reported.
#[test]
fn an_arm_where_nothing_reached_the_model_aborts_the_run() {
    let dead = vec![
        vec![run(false, Some("LLM stream error: connection refused"))],
        vec![run(false, Some("LLM stream error: connection refused"))],
    ];

    let error = empty_column_error(EvalArm::Gglib, &dead).expect("aborts");
    assert!(error.contains("gglib"), "names the arm: {error}");
    assert!(error.contains("all 2 runs"), "names the count: {error}");
    assert!(
        error.contains("connection refused"),
        "quotes the upstream's own reason, which is what the operator acts on: {error}"
    );
}

/// One surviving measurement is enough to make the arm a real, if bad,
/// observation — the eval must not throw away a run over a transient blip.
#[test]
fn a_single_measured_run_keeps_the_arm() {
    let mostly_dead = vec![
        vec![run(false, Some("LLM stream error"))],
        vec![run(false, None)],
    ];

    assert!(empty_column_error(EvalArm::Raw, &mostly_dead).is_none());
}

/// **The distinction the check turns on.** An arm that failed every task
/// while talking to the model perfectly well is a real result — a score of
/// zero is the honest report of a model that got everything wrong.
#[test]
fn an_arm_that_merely_failed_everything_is_not_empty() {
    let all_wrong = vec![vec![run(false, None)], vec![run(false, None)]];

    assert!(empty_column_error(EvalArm::Gglib, &all_wrong).is_none());
}

/// An arm with no runs planned has nothing to be empty of, and must not be
/// reported as an upstream failure.
#[test]
fn an_arm_with_no_runs_does_not_abort() {
    assert!(empty_column_error(EvalArm::Control, &[]).is_none());
    assert!(empty_column_error(EvalArm::Control, &[vec![]]).is_none());
}

/// A demanded call sends `tool_choice: "required"`; an irrelevance task
/// must not, or the model would be forced to call a tool the task
/// expects it to abstain from.
#[test]
fn tool_choice_follows_the_expected_outcome() {
    let demanding = call_task(ExpectedOutcome::ToolCalls {
        calls: vec![ExpectedCall {
            name: "f".into(),
            required_args: serde_json::Map::new(),
            ordered: false,
            depends_on_result: false,
        }],
    });
    let abstaining = call_task(ExpectedOutcome::NoToolCall);

    assert!(demands_tool_call(&demanding));
    assert!(!demands_tool_call(&abstaining));
}
