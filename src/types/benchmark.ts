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

// ─── Enumerations ────────────────────────────────────────────────────────────

export type BenchmarkRunType = 'compare' | 'perf' | 'tune';
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

export interface ModelBenchmarkSummary {
  model_id: number;
  best_tg_tps?: number | null;
  best_pp_tps?: number | null;
  latest_tg_tps?: number | null;
  latest_pp_tps?: number | null;
  latest_backend?: string | null;
  perf_run_count: number;
  compare_run_count: number;
  last_benchmarked_at: string;
  updated_at: string;
}

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
}

/** Weights combining per-candidate metrics into a composite score. */
export interface ScoreWeights {
  tool_accuracy: number;
  loop_avoidance: number;
  task_completion: number;
  speed: number;
}

export interface TuneConfig {
  model_id: number;
  task_suite: TaskSuite;
  sweep: SweepSpec;
  seed_from_gguf: boolean;
  seed_from_family_presets: boolean;
  weights: ScoreWeights;
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
  | { kind: 'gguf_author_default' }
  | { kind: 'family_preset'; family: string };

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

// ─── Agentic A/B Eval (raw vs gglib) ─────────────────────────────────────────

/** Which arm of the A/B eval a task ran under. */
export type EvalArm = 'raw' | 'gglib';

/** One arm's aggregate scores. Mirrors `gglib_core::domain::benchmark::agentic::ArmScores`. */
export interface ArmScores {
  tool_accuracy: number;
  /**
   * Fraction of loop-eligible tasks that tripped neither guard. `null` when no
   * task ever reached a second tool-call batch — the axis was not measured,
   * which is distinct from a perfect 1.0. Read with `loop_eligible`.
   */
  loop_avoidance?: number | null;
  /** Denominator behind `loop_avoidance`: how many tasks risked a loop. */
  loop_eligible: number;
  task_completion: number;
  composite: number;
  tg_tps?: number | null;
  /** Suite-wide generated tokens; `null` when no task reported usage. */
  total_completion_tokens?: number | null;
  /** Suite-wide wall clock, unfiltered. */
  total_wall_ms: number;
  /** Mean time to first tool call, over the tasks that made one. */
  mean_time_to_first_tool_call_ms?: number | null;
}

/**
 * Per-axis `gglib − raw` difference, plus the efficiency ratios.
 *
 * Quality axes are differences (positive means gglib scored higher);
 * efficiency figures are ratios `raw ÷ gglib` (above 1.0 means gglib did
 * better), because lower is better on those and the gaps are multiplicative.
 */
export interface ArmDelta {
  tool_accuracy: number;
  /** `null` unless both arms measured the axis. */
  loop_avoidance?: number | null;
  task_completion: number;
  composite: number;
  /** Suite wall-time speedup, `raw ÷ gglib`. */
  wall_time_speedup?: number | null;
  /** Completion-token ratio, `raw ÷ gglib`. */
  completion_token_ratio?: number | null;
}

/** One task's outcome under both arms. */
export interface AgenticTaskComparison {
  task_id: string;
  category: TaskCategory;
  raw: TuneTaskResult;
  gglib: TuneTaskResult;
}

/** The complete raw-vs-gglib report. Mirrors `AgenticEvalReport`. */
export interface AgenticEvalReport {
  model_name: string;
  quantization?: string | null;
  param_count_b: number;
  ctx_size: number;
  raw: ArmScores;
  gglib: ArmScores;
  delta: ArmDelta;
  tasks: AgenticTaskComparison[];
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
