//! Tests for the per-arm rollups in `agentic_rollup.rs`.

use super::super::agentic_tests::run;
use super::*;

/// A run that generated `chars` of reasoning and executed a batch of
/// `batch` tool calls, with a first-call latency of `first_call_ms`.
fn generating(chars: u64, batch: usize, first_call_ms: u64) -> TuneTaskResult {
    let mut r = run(true, None);
    r.time_to_first_tool_call_ms = Some(first_call_ms);
    r.generated = GeneratedOutput {
        reasoning_chars: chars,
        answer_chars: 10,
        llm_calls: 2,
        max_tool_calls_in_batch: batch,
        system_warnings: 0,
    };
    r
}

/// **The point of taking a maximum rather than a mean.** A constrained-decode
/// runaway is one batch among dozens of ordinary runs; averaged, it vanishes
/// into the arm and the report says nothing happened.
#[test]
fn a_single_runaway_batch_survives_the_arm_rollup() {
    let results: Vec<TuneTaskResult> = (0..20)
        .map(|_| generating(100, 1, 1_000))
        .chain(std::iter::once(generating(100, 512, 1_000)))
        .collect();

    let rolled = aggregate_generated(&results);
    assert_eq!(
        rolled.max_tool_calls_in_batch, 512,
        "the widest batch is the finding; a mean over 21 runs would report 25"
    );
    // The additive fields still add.
    assert_eq!(rolled.reasoning_chars, 2_100);
    assert_eq!(rolled.llm_calls, 42);
}

/// A run that never reached the model generated nothing, and folding its
/// zeros in would understate the arm exactly where it was least healthy —
/// the same population rule `measured_wall_ms` follows.
#[test]
fn an_unmeasured_run_contributes_nothing_to_the_rollup() {
    let mut dead = generating(9_999, 99, 1_000);
    dead.unmeasured = Some("SSE byte-stream error".to_owned());

    let rolled = aggregate_generated(&[generating(100, 2, 1_000), dead]);
    assert_eq!(rolled.reasoning_chars, 100);
    assert_eq!(rolled.max_tool_calls_in_batch, 2);
}

/// **The metric that flipped.** Run 1 excluded the five stalled runs because
/// they never called a tool; run 2 includes them at ~950s. The mean of that
/// population lands near 94s and describes neither the 46 fast runs nor the
/// 5 slow ones — so the median is reported beside it, and the gap between
/// them is what says the arm is not one population.
#[test]
fn the_median_first_call_survives_a_tail_that_wrecks_the_mean() {
    let results: Vec<TuneTaskResult> = (0..46)
        .map(|_| generating(10, 1, 1_029))
        .chain((0..5).map(|_| generating(10, 1, 950_000)))
        .collect();

    let median = median_time_to_first_tool_call_ms(&results).expect("51 callers");
    let mean = mean_time_to_first_tool_call_ms(&results).expect("51 callers");

    assert!(
        (median - 1_029.0).abs() < f64::EPSILON,
        "the typical run took ~1s, not {median:.0}ms"
    );
    assert!(
        mean > 90_000.0,
        "the mean is dragged past 90s by five runs: {mean:.0}ms"
    );
}

/// An even-length sample takes the midpoint rather than arbitrarily
/// preferring one side — a 2-run arm is the common case for the isolated
/// single-task repro.
#[test]
fn an_even_sample_medians_to_the_midpoint() {
    let results = vec![generating(10, 1, 100), generating(10, 1, 200)];
    let median = median_time_to_first_tool_call_ms(&results).expect("two callers");
    assert!((median - 150.0).abs() < f64::EPSILON, "got {median}");
}

/// Abstaining tasks are not zeros. An `Irrelevance` task correctly never
/// calls a tool, and counting that as an instant first call would flatter
/// whichever arm abstained most — the median must share the mean's
/// population exactly.
#[test]
fn a_task_that_never_called_a_tool_is_not_a_zero() {
    let mut abstained = generating(10, 0, 0);
    abstained.time_to_first_tool_call_ms = None;

    let results = vec![generating(10, 1, 500), abstained];
    let median = median_time_to_first_tool_call_ms(&results).expect("one caller");
    assert!(
        (median - 500.0).abs() < f64::EPSILON,
        "the abstaining task must not pull the median to 250: got {median}"
    );
}
