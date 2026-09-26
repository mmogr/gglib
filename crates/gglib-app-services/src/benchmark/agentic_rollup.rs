//! How one arm's per-task, per-seed results roll up into its [`ArmScores`].
//!
//! Everything here is a pure function of the results it is handed: no model,
//! no network, no report assembly.

use gglib_core::domain::benchmark::agentic::ArmScores;
use gglib_core::domain::benchmark::tune::config::ScoreWeights;
use gglib_core::domain::benchmark::tune::result::{GeneratedOutput, TuneTaskResult};

use super::super::tune::{axis_scores, compute_composite_score, throughput_tps};

/// Flatten per-task, per-seed results into one list.
///
/// Every mean below is taken over the flat list rather than over per-task
/// means. With a balanced design — and this one is balanced by construction,
/// every task running every seed — the two are arithmetically identical, and
/// the flat form keeps one code path shared with the single-seed sweep.
pub(super) fn flatten(per_task: &[Vec<TuneTaskResult>]) -> Vec<TuneTaskResult> {
    per_task.iter().flatten().cloned().collect()
}

/// Aggregate one arm's task results into [`ArmScores`].
pub(super) fn arm_scores(
    results: &[TuneTaskResult],
    weights: &ScoreWeights,
    seeds: usize,
    tasks: usize,
) -> ArmScores {
    let axes = axis_scores(results);
    let composite = compute_composite_score(results, weights);
    ArmScores {
        seeds,
        runs: tasks * seeds,
        unmeasured_runs: results.iter().filter(|r| !r.is_measured()).count(),
        transport_retries: results.iter().map(|r| r.transport_retries).sum(),
        tool_accuracy: axes.as_ref().map_or(0.0, |a| a.tool_accuracy),
        loop_avoidance: axes.as_ref().and_then(|a| a.loop_avoidance),
        loop_eligible: axes.as_ref().map_or(0, |a| a.loop_eligible),
        task_completion: axes.as_ref().map_or(0.0, |a| a.task_completion),
        composite,
        tg_tps: throughput_tps(results),
        total_completion_tokens: total_completion_tokens(results),
        total_wall_ms: results.iter().map(|r| r.latency_ms).sum(),
        measured_wall_ms: results
            .iter()
            .filter(|r| r.is_measured())
            .map(|r| r.latency_ms)
            .sum(),
        mean_time_to_first_tool_call_ms: mean_time_to_first_tool_call_ms(results),
        median_time_to_first_tool_call_ms: median_time_to_first_tool_call_ms(results),
        generated: aggregate_generated(results),
    }
}

/// Roll the per-run generation shapes up to the arm.
///
/// Measured runs only — an unmeasured run generated nothing, and its zeros
/// would understate the arm exactly where it was least healthy.
///
/// `max_tool_calls_in_batch` takes the arm-wide maximum rather than a sum or a
/// mean. One runaway batch among sixty-three ordinary runs is the whole signal,
/// and both other aggregations would bury it.
pub(super) fn aggregate_generated(results: &[TuneTaskResult]) -> GeneratedOutput {
    results
        .iter()
        .filter(|r| r.is_measured())
        .fold(GeneratedOutput::default(), |mut acc, r| {
            acc.reasoning_chars += r.generated.reasoning_chars;
            acc.answer_chars += r.generated.answer_chars;
            acc.llm_calls += r.generated.llm_calls;
            acc.system_warnings += r.generated.system_warnings;
            acc.max_tool_calls_in_batch = acc
                .max_tool_calls_in_batch
                .max(r.generated.max_tool_calls_in_batch);
            acc
        })
}

/// Suite-wide completion tokens. `None` only when no task reported usage,
/// which stays distinct from a measured zero.
pub(super) fn total_completion_tokens(results: &[TuneTaskResult]) -> Option<u64> {
    let mut total: Option<u64> = None;
    for tokens in results.iter().filter_map(|r| r.completion_tokens) {
        total = Some(total.unwrap_or(0) + tokens);
    }
    total
}

/// Mean time to first tool call across the tasks that made one.
///
/// Averaged over callers only: an `Irrelevance` task correctly never calls a
/// tool, and folding its absence in as a zero would flatter whichever arm
/// abstained most.
pub(super) fn mean_time_to_first_tool_call_ms(results: &[TuneTaskResult]) -> Option<f64> {
    let samples: Vec<u64> = results
        .iter()
        .filter_map(|r| r.time_to_first_tool_call_ms)
        .collect();
    if samples.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    Some(samples.iter().sum::<u64>() as f64 / samples.len() as f64)
}

/// Median time to first tool call across the tasks that made one.
///
/// Same population as [`mean_time_to_first_tool_call_ms`], and reported beside
/// it rather than instead of it. The mean stopped describing this arm the
/// moment a few runs generated for a quarter of an hour before acting; the
/// median describes the typical run, and the gap between the two is what says a
/// handful of runs behaved nothing like the rest.
///
/// Even-length samples take the mean of the two middle values, so a 2-run arm
/// reports the midpoint rather than arbitrarily picking a side.
pub(super) fn median_time_to_first_tool_call_ms(results: &[TuneTaskResult]) -> Option<f64> {
    let mut samples: Vec<u64> = results
        .iter()
        .filter_map(|r| r.time_to_first_tool_call_ms)
        .collect();
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let mid = samples.len() / 2;
    #[allow(clippy::cast_precision_loss)]
    Some(if samples.len() % 2 == 1 {
        samples[mid] as f64
    } else {
        (samples[mid - 1] as f64 + samples[mid] as f64) / 2.0
    })
}

#[cfg(test)]
#[path = "agentic_rollup_tests.rs"]
mod agentic_rollup_tests;
