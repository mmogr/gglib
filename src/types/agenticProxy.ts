// ============================================================================
// Agentic Eval: the Proxy Pair
// ============================================================================
//
// The proxy arm and its raw-auto baseline, as the agentic report carries
// them. Split from `agenticEval.ts`; `benchmark.ts` re-exports both.
//
// @module types/agenticProxy

import type { ArmDelta, ArmScores, PairedEffect } from './agenticEval';
import type { TuneTaskResult } from './benchmark';
import type { LoopGuardMode } from './generated/LoopGuardMode';
import type { ModelDefectCounts } from './generated/ModelDefectCounts';

/**
 * The proxy arm beside its raw-auto baseline, and what the in-process proxy
 * counted while the arm ran. Mirrors
 * `gglib_core::domain::benchmark::agentic::ProxyArms`. Both arms open with
 * `tool_choice: "auto"`; they are compared with each other, never with
 * `raw` or `gglib`.
 */
export interface ProxyArms {
  raw_auto: ArmScores;
  proxy: ArmScores;
  /**
   * Per-axis `proxy − raw_auto`: everything the proxy does, its request
   * pipeline included. When withheld, its `raw` count is raw-auto's
   * unmeasured runs and its `gglib` count the proxy arm's.
   */
  delta: ArmDelta;
  /** Paired per-(task, seed), `proxy − raw_auto`; null when no pair was measured on both sides. */
  paired: PairedEffect | null;
  /**
   * The proxy arm's own totals. Whether repair did anything is read here, not
   * from the scores. With repair on, zero attempts means no call the proxy could judge broke
   * its schema.
   */
  defects: ModelDefectCounts;
  settings: ProxyArmSettings;
  tasks: ProxyTaskRuns[];
}

/** Two of the fixed settings the in-process proxy ran under. */
export interface ProxyArmSettings {
  /**
   * False only when `GGLIB_DISABLE_TOOL_REPAIR` was set in the environment of
   * the process running the eval, as read when the arm's proxy started.
   */
  tool_call_repair: boolean;
  loop_guard_mode: LoopGuardMode;
}

/** One task's per-seed runs under the proxy pair, in seed order. */
export interface ProxyTaskRuns {
  task_id: string;
  raw_auto: TuneTaskResult[];
  proxy: TuneTaskResult[];
}
