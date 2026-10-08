// ============================================================================
// Benchmark Domain Types
// ============================================================================
//
// What the benchmark routes answer with is generated from the Rust types and
// re-exported here, so importers read every benchmark shape from this module.
//
// What is written by hand is what Rust derives no binding for: the four
// request configs, the task-suite schema a custom suite file is parsed into,
// the SSE event union and the apply gate's verdict. Their serde config:
//   - BenchmarkEvent: `#[serde(tag = "type", rename_all = "snake_case")]`
//   - BenchmarkModelResult: `#[serde(tag = "kind", rename_all = "snake_case")]`
//   - `TaskSuite`/`ExpectedOutcome` use `#[serde(tag = "...")]` internal
//     tagging (see each type below for its tag key)
//
// @module types/benchmark

import type { AgenticEvalReport } from './generated/AgenticEvalReport';
import type { ModelCompareResult } from './generated/ModelCompareResult';
import type { ModelPerfResult } from './generated/ModelPerfResult';
import type { PairedEffect } from './generated/PairedEffect';
import type { TaskCategory } from './generated/TaskCategory';
import type { TuneCandidateResult } from './generated/TuneCandidateResult';
import type { EvalArm } from './agenticEval';

export type * from './agenticEval';

// ─── Generated ───────────────────────────────────────────────────────────────

export type { BenchmarkRun } from './generated/BenchmarkRun';
export type { GeneratedOutput } from './generated/GeneratedOutput';
/** Response of `GET /api/benchmark/runs`. */
export type { ListRunsResponse } from './generated/ListRunsResponse';
/** Response of `GET /api/models/{id}/agentic-history`, most recent first. */
export type { ModelAgenticHistoryResponse } from './generated/ModelAgenticHistoryResponse';
export type { ModelCompareResult, ModelPerfResult, TuneCandidateResult };
export type { TuneTaskResult } from './generated/TuneTaskResult';

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
