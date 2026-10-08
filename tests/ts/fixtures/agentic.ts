/**
 * An agentic eval report, as `GET /api/models/{id}/agentic-history` and the
 * `agentic_eval_complete` event send one.
 *
 * None of the report's Rust structs (`AgenticEvalReport`, `ArmScores`,
 * `ArmDelta`, `TuneTaskResult`, `GeneratedOutput`) skips a field when it
 * serializes, so a report names every key: an arm that did not run is `null`,
 * a list nothing filled is `[]`, a count nothing counted is `0`.
 *
 * The baselines are a report in which nothing was measured. Where Rust gives
 * a field a serde default the baseline is that default, so a report stored
 * before the field existed reads back the same way (`ArmScores.seeds`, at 1,
 * is the one that is not zero or empty).
 */
import type {
  AgenticEvalReport,
  ArmDelta,
  ArmScores,
  TuneTaskResult,
} from '../../../src/types/benchmark';

/** One arm's scores, with nothing measured unless `overrides` says so. */
export function armScores(overrides: Partial<ArmScores> = {}): ArmScores {
  return {
    tool_accuracy: 0,
    loop_avoidance: null,
    loop_eligible: 0,
    task_completion: 0,
    composite: 0,
    tg_tps: null,
    total_completion_tokens: null,
    total_wall_ms: 0,
    measured_wall_ms: 0,
    mean_time_to_first_tool_call_ms: null,
    median_time_to_first_tool_call_ms: null,
    seeds: 1,
    runs: 0,
    unmeasured_runs: 0,
    transport_retries: 0,
    generated: {
      reasoning_chars: 0,
      answer_chars: 0,
      llm_calls: 0,
      max_tool_calls_in_batch: 0,
      system_warnings: 0,
    },
    ...overrides,
  };
}

/** A delta with every axis absent, which is `null` on the wire. */
export function armDelta(overrides: Partial<ArmDelta> = {}): ArmDelta {
  return {
    tool_accuracy: null,
    loop_avoidance: null,
    task_completion: null,
    composite: null,
    withheld: null,
    wall_time_speedup: null,
    completion_token_ratio: null,
    ...overrides,
  };
}

/** One task run that reached the model and was not retried. */
export function taskResult(overrides: Partial<TuneTaskResult> = {}): TuneTaskResult {
  return {
    task_id: 't',
    category: 'single_call',
    passed: true,
    tool_match_score: 1,
    loop_detected: false,
    stagnation_detected: false,
    iterations: 1,
    latency_ms: 0,
    completion_tokens: null,
    time_to_first_tool_call_ms: null,
    detail: null,
    unmeasured: null,
    transport_retries: 0,
    generated: armScores().generated,
    ...overrides,
  };
}

/** A report of the two main arms alone: no control, no A/A arm, no proxy pair. */
export function agenticReport(overrides: Partial<AgenticEvalReport> = {}): AgenticEvalReport {
  return {
    model_name: 'm',
    quantization: null,
    param_count_b: 4,
    ctx_size: 8192,
    raw: armScores(),
    gglib: armScores(),
    delta: armDelta(),
    tasks: [],
    seeds: [],
    control: null,
    raw_replicate: null,
    replicate_seeds: [],
    raw_replicates: [],
    replicate_seed_sets: [],
    paired: null,
    proxy: null,
    ...overrides,
  };
}
