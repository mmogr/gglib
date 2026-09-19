// ============================================================================
// Benchmark Domain Types
// ============================================================================
//
// Mirrors the Rust `gglib-app-services::benchmark` domain types.
// Serde config on the Rust side:
//   - BenchmarkEvent: `#[serde(tag = "type", rename_all = "snake_case")]`
//   - BenchmarkModelResult: `#[serde(tag = "kind", rename_all = "snake_case")]`
//   - All structs: `#[serde(rename_all = "snake_case")]`
//   - Tune's `TaskSuite`/`ExpectedOutcome`/`CandidateSource` use
//     `#[serde(tag = "...")]` internal tagging (see each type below for its
//     tag key); `InferenceConfig` is the one exception that serializes
//     camelCase (`#[serde(rename_all = "camelCase")]`) — reused as-is from
//     `../types` (this file and `../types/index.ts` have a type-only mutual
//     import, which TypeScript permits and erases at compile time).
//
// @module types/benchmark

import type { InferenceConfig } from './index';
import type { AgenticEvalReport, EvalArm, PairedEffect } from './agenticEval';

export type * from './agenticEval';

// ─── Enumerations ────────────────────────────────────────────────────────────

export type BenchmarkRunType = 'compare' | 'perf' | 'tune' | 'agentic';
export type BenchmarkRunStatus = 'running' | 'complete' | 'failed';

// ─── Domain Entities ─────────────────────────────────────────────────────────

export interface BenchmarkRun {
  id: number;
  run_type: BenchmarkRunType;
  status: BenchmarkRunStatus;
  model_ids: number[];
  prompt_text?: string | null;
  system_prompt?: string | null;
  config_json?: string | null;
  /**
   * The gate's outcome record for a tune run (JSON ApplyRecord: verdict +
   * applied config + displaced defaults). Refusals leave records too; null
   * on runs never judged and on non-tune runs.
   */
  applied_json?: string | null;
  error?: string | null;
  created_at: string;       // ISO 8601 UTC
  completed_at?: string | null;
}

export interface ModelCompareResult {
  id?: number | null;
  model_id: number;
  run_id?: number | null;
  prompt_text: string;
  system_prompt?: string | null;
  response_text: string;
  was_truncated: boolean;
  prompt_tokens?: number | null;
  completion_tokens?: number | null;
  prompt_ms?: number | null;
  generation_ms?: number | null;
  prompt_tps?: number | null;
  generation_tps?: number | null;
  created_at: string;
}

export interface ModelPerfResult {
  id?: number | null;
  model_id: number;
  run_id?: number | null;
  pp_tps: number;
  tg_tps: number;
  pp_tokens: number;
  tg_tokens: number;
  backend?: string | null;
  ngl?: number | null;
  context_size?: number | null;
  repetitions: number;
  created_at: string;
}

/**
 * A model's cached benchmark headline — the `benchmarkSummary` on a library
 * row and a model detail.
 *
 * Five fields are required nullables rather than optional keys — the four
 * `*_tps` figures and `latest_backend`. The row is built from a query that
 * names every column, so a model benchmarked only one way sends `null` for
 * the other rather than omitting the key.
 */
import type { ModelBenchmarkSummary } from './generated/ModelBenchmarkSummary';
export type { ModelBenchmarkSummary };

// ─── Request Configs ─────────────────────────────────────────────────────────

export interface CompareConfig {
  model_ids: number[];
  prompt: string;
  system_prompt?: string | null;
  inference?: Record<string, unknown> | null;
  ctx_size?: number | null;
}

export interface PerfConfig {
  model_ids: number[];
  pp_tokens?: number | null;
  tg_tokens?: number | null;
  repetitions?: number | null;
}

// ─── Tune: task schema ──────────────────────────────────────────────────────

/**
 * BFCL-style category (plus `long_context`, gglib-specific) a
 * {@link TuneTask} belongs to.
 */
export type TaskCategory =
  | 'single_call'
  | 'parallel_call'
  | 'multi_turn'
  | 'irrelevance'
  | 'long_context';

/**
 * One expected tool call within a task's `tool_calls` outcome. Matching is
 * AST-style: `name` must match exactly, `required_args` must be a subset of
 * the recorded arguments (extra args ignored), values compared structurally
 * (not string diff).
 */
export interface ExpectedCall {
  name: string;
  required_args: Record<string, unknown>;
  ordered: boolean;
}

/**
 * What a task expects the agent loop to do.
 * Serde: `#[serde(tag = "kind", rename_all = "snake_case")]`.
 */
export type ExpectedOutcome =
  | { kind: 'tool_calls'; calls: ExpectedCall[] }
  | { kind: 'no_tool_call' };

/**
 * A single scripted agentic scenario evaluated during a tune run. This is
 * the exact shape of one element in a custom task-suite JSON file/upload
 * (see `gglib-core/assets/tune_default_suite.json` for a full example) —
 * the CLI (`--task-suite path.json`) and the GUI (file upload, parsed
 * client-side) both parse a plain `TuneTask[]` array in this shape.
 */
export interface TuneTask {
  id: string;
  category: TaskCategory;
  system_prompt?: string | null;
  /**
   * Simulated prior conversation turns injected before `user_prompt`
   * (`long_context` tasks only). Each entry is a `ChatMessage`-shaped
   * object mirroring the Rust `AgentMessage` wire format:
   * `{"role":"user","content":"..."}`,
   * `{"role":"assistant","content":"...","tool_calls":[...]}`,
   * `{"role":"tool","tool_call_id":"...","content":"..."}`.
   */
  history?: Record<string, unknown>[] | null;
  user_prompt: string;
  tools: {
    name: string;
    description?: string | null;
    input_schema?: Record<string, unknown> | null;
  }[];
  expected: ExpectedOutcome;
}

/**
 * The set of tasks a tune run evaluates each candidate against.
 * Serde: `#[serde(tag = "source", rename_all = "snake_case")]`.
 *
 * `Custom` carries the exact same `TuneTask[]` array shape whether it
 * originates from the CLI (`--task-suite path.json`, parsed locally) or the
 * GUI (a file upload parsed client-side into a plain array, then wrapped
 * into this shape before being sent as part of the request body) — one
 * shared schema, no divergent ingestion paths.
 */
export type TaskSuite = { source: 'default' } | { source: 'custom'; tasks: TuneTask[] };

// ─── Tune: configuration ────────────────────────────────────────────────────

/** Per-dimension candidate value lists; cartesian product forms the grid. */
export interface SweepSpec {
  temperature: number[];
  top_p: number[];
  top_k: number[];
  min_p: number[];
  repeat_penalty: number[];
  /** DRY multiplier; `0.0` disables DRY, so it is a meaningful candidate. */
  dry_multiplier: number[];
  /** Dynatemp half-range; `0.0` disables, so off-vs-on sweeps in one run. */
  dynatemp_range: number[];
  /** Dynatemp exponent; meaningful only beside a non-zero range. */
  dynatemp_exponent: number[];
  /** Top-n-sigma; `-1.0` disables, so off-vs-on sweeps in one run. */
  top_n_sigma: number[];
}

/** Weights combining per-candidate metrics into a composite score. */
export interface ScoreWeights {
  tool_accuracy: number;
  loop_avoidance: number;
  task_completion: number;
}

export interface TuneConfig {
  model_id: number;
  task_suite: TaskSuite;
  sweep: SweepSpec;
  seed_from_family_presets: boolean;
  /** Omit to accept the server's default; see {@link AgenticEvalConfig.weights}. */
  weights?: ScoreWeights;
  prune_fraction: number;
  ctx_size?: number | null;
}

// ─── Tune: results ────────────────────────────────────────────────────────────

/**
 * Where a tune candidate's sampling settings came from.
 * Serde: `#[serde(tag = "kind", rename_all = "snake_case")]`.
 */
export type CandidateSource =
  | { kind: 'user_grid' }
  | { kind: 'family_preset'; family: string }
  | { kind: 'incumbent' }
  | { kind: 'incumbent_calibration' };

/**
 * The shape of what a run generated, as opposed to how much.
 *
 * A token total and a wall time cannot distinguish a model thinking at length
 * from one failing to stop, and those call for opposite responses.
 */
export interface GeneratedOutput {
  /**
   * Characters emitted as reasoning (chain-of-thought).
   *
   * Only meaningful when the upstream splits thinking into its own
   * `reasoning_content` field. Without that, a reasoning model's thinking is
   * counted as `answer_chars` instead — so `0` here beside a large
   * `answer_chars` means either "did not think" or "thought, unobservably".
   */
  reasoning_chars?: number;
  /** Characters emitted as ordinary answer text, across every turn. */
  answer_chars?: number;
  /**
   * Requests actually sent to the model. Distinct from `iterations`, which
   * counts only tool-executing turns.
   */
  llm_calls?: number;
  /**
   * The largest single batch of tool calls any one turn executed — the
   * fingerprint of a constrained-decoding runaway, which scoring cannot reveal
   * because extra unrequested calls cost nothing.
   */
  max_tool_calls_in_batch?: number;
  /** Recoverable conditions the loop reported, chiefly over-wide call batches. */
  system_warnings?: number;
}

/** Result of evaluating one task against one candidate's sampling settings. */
export interface TuneTaskResult {
  task_id: string;
  category: TaskCategory;
  passed: boolean;
  tool_match_score: number;
  loop_detected: boolean;
  stagnation_detected: boolean;
  iterations: number;
  latency_ms: number;
  completion_tokens?: number | null;
  /** Time to the model's first tool call; `null` when it never called one. */
  time_to_first_tool_call_ms?: number | null;
  detail?: string | null;
  /**
   * Why this run is **not a measurement of the model**, when it is not one.
   *
   * `null` on every run that reached the model, including every way of doing
   * badly — a wrong call, a detected loop, an exhausted budget all score
   * honestly. A non-null reason means the request never produced a response to
   * score, so this row's `passed: false` and `tool_match_score: 0` are the
   * absence of a measurement rather than a bad one, and must not be rendered
   * as a failure the model is responsible for.
   */
  unmeasured?: string | null;
  /** What the model generated, as opposed to how much. */
  generated?: GeneratedOutput;
}

/**
 * Result of evaluating one candidate's sampling settings. `config` is a
 * plain {@link InferenceConfig} — pass it directly to `updateModel({
 * inferenceDefaults: result.config })` to apply it, no field mapping needed.
 */
export interface TuneCandidateResult {
  config: InferenceConfig;
  source: CandidateSource;
  task_results: TuneTaskResult[];
  composite_score: number;
  pruned: boolean;
  tg_tps?: number | null;
}

// ─── SSE Event Discriminated Union ───────────────────────────────────────────

/** Payload of a `model_complete` event; tagged by `kind`. */
export type BenchmarkModelResult =
  | ({ kind: 'compare' } & ModelCompareResult)
  | ({ kind: 'perf' } & ModelPerfResult);

/**
 * Discriminated union of all SSE events emitted by the benchmark SSE stream.
 * Serde: `#[serde(tag = "type", rename_all = "snake_case")]`
 */
export type BenchmarkEvent =
  | { type: 'model_started'; model_id: number; model_name: string; position: number; total: number }
  | { type: 'model_text_delta'; model_id: number; text: string }
  | { type: 'model_complete'; model_id: number; result: BenchmarkModelResult }
  | { type: 'model_failed'; model_id: number; model_name: string; error: string }
  | { type: 'run_complete'; run_id: number }
  | { type: 'run_failed'; error: string }
  | { type: 'tune_candidate_started'; candidate_index: number; total: number }
  | { type: 'tune_task_complete'; candidate_index: number; task_id: string; passed: boolean }
  | { type: 'tune_pruned'; candidate_index: number; reason: string }
  | { type: 'tune_candidate_complete'; result: TuneCandidateResult }
  | { type: 'agentic_arm_started'; arm: EvalArm; total_tasks: number }
  | { type: 'agentic_task_complete'; arm: EvalArm; task_id: string; passed: boolean }
  | { type: 'agentic_eval_complete'; report: AgenticEvalReport };

/**
 * The apply gate's decision on one completed tune run. Mirrors
 * `gglib_core::domain::benchmark::tune::apply::ApplyVerdict`
 * (`#[serde(tag = "verdict", rename_all = "snake_case")]`). Refusals are
 * first-class outcomes carrying the evidence that was missing or contrary.
 */
export type ApplyVerdict =
  | {
      verdict: 'apply';
      winner_composite: number;
      incumbent_mean: number;
      margin: number;
      drift: number;
      paired?: PairedEffect | null;
    }
  | { verdict: 'incumbent_stands'; incumbent_mean: number }
  | { verdict: 'within_drift'; margin: number; drift: number }
  | { verdict: 'paired_disagrees'; wins: number; losses: number }
  | { verdict: 'uncalibrated' }
  | { verdict: 'contaminated'; unmeasured_runs: number };

/** Response of `POST /api/benchmark/tune/{run_id}/apply`. */
export interface ApplyOutcome {
  verdict: ApplyVerdict;
  model_id: number;
  /** Whether the model was actually written. */
  applied: boolean;
}

// ─── API Response Shapes ─────────────────────────────────────────────────────

export interface ListBenchmarkRunsResponse {
  runs: BenchmarkRun[];
}

export interface GetBenchmarkRunResponse {
  run: BenchmarkRun;
}

export interface ModelBenchmarkHistoryResponse {
  summary?: ModelBenchmarkSummary | null;
  compare_history: ModelCompareResult[];
  perf_history: ModelPerfResult[];
}

export interface ModelTuneHistoryResponse {
  results: TuneCandidateResult[];
}

/** Response for `GET /api/models/{id}/agentic-history`, most recent first. */
export interface ModelAgenticHistoryResponse {
  reports: AgenticEvalReport[];
}
