// ============================================================================
// Agentic A/B Eval Types
// ============================================================================
//
// Split from `benchmark.ts`, which re-exports everything here, so importers
// keep reading these from `types/benchmark`. The report and everything in it
// is generated from the Rust types; the arm names and the request config,
// which Rust derives no binding for, are written by hand.
//
// @module types/agenticEval

import type { ScoreWeights, TaskSuite } from './benchmark';

export type { AgenticEvalReport } from './generated/AgenticEvalReport';
export type { AgenticTaskComparison } from './generated/AgenticTaskComparison';
export type { ArmDelta } from './generated/ArmDelta';
export type { ArmScores } from './generated/ArmScores';
/** The proxy arm beside its raw-auto baseline; compared with each other, never with `raw` or `gglib`. */
export type { ProxyArms } from './generated/ProxyArms';

// ─── Agentic A/B Eval (raw vs gglib) ─────────────────────────────────────────

/**
 * Which arm of the A/B eval a task ran under.
 *
 * Two of the four measure the eval rather than the pipeline: `raw_replicate`
 * re-runs `raw` on a disjoint seed set (an A/A test, whose gap is the eval's
 * own drift), and `control` runs the pipeline with sampling deliberately broken
 * and must score far below `gglib`.
 *
 * `raw_auto` and `proxy` run only when asked for (`include_proxy`), and are
 * compared with each other: every turn of `proxy` goes through a real
 * gglib-proxy, and `raw_auto` is its baseline. Both open with
 * `tool_choice: "auto"`, under which the proxy judges every call whose schema
 * it can judge.
 */
export type EvalArm = 'raw' | 'gglib' | 'raw_auto' | 'proxy' | 'raw_replicate' | 'control';

/**
 * Request body for `POST /api/benchmark/agentic`. Mirrors
 * `gglib_core::domain::benchmark::agentic::AgenticEvalConfig`; every field
 * except `model_id` and `task_suite` is serde-defaulted server-side.
 */
export interface AgenticEvalConfig {
  model_id: number;
  task_suite: TaskSuite;
  /**
   * Omit to accept the server's default. `ScoreWeights::default()` is the
   * source of truth; it currently reads
   * `{ tool_accuracy: 0.4, loop_avoidance: 0.3, task_completion: 0.2 }`.
   * Sending a copy of those numbers pins them, which is the drift that
   * removing the `speed` weight was about.
   */
  weights?: ScoreWeights;
  ctx_size?: number | null;
  /**
   * Server default: [12345, 67890, 11111]. An EMPTY array is meaningful —
   * one unseeded run per task, which carries full decode variance.
   */
  seeds?: number[];
  /** Server default true — run the sampling-broken positive control arm. */
  include_control?: boolean;
  /** Server default true — re-run the raw arm on disjoint seeds (the A/A drift floor). */
  replicate_raw?: boolean;
  /**
   * Server default 1. Each extra pair re-runs the raw arm on another derived
   * seed set, and the drift floor becomes the mean over every pairwise gap.
   */
  replicate_pairs?: number;
  /** Server default 1; clamped into 1..=seeds.len() server-side. */
  control_seeds?: number;
  /** Server default false — also run the proxy arm and its raw-auto baseline. */
  include_proxy?: boolean;
}
