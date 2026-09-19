// ============================================================================
// Agentic A/B Eval Types
// ============================================================================
//
// Split from `benchmark.ts`, which re-exports everything here, so importers
// keep reading these from `types/benchmark`. Serde conventions are the ones
// that file's header describes.
//
// @module types/agenticEval

import type {
  GeneratedOutput,
  ScoreWeights,
  TaskCategory,
  TaskSuite,
  TuneTaskResult,
} from './benchmark';

// ─── Agentic A/B Eval (raw vs gglib) ─────────────────────────────────────────

/**
 * Which arm of the A/B eval a task ran under.
 *
 * Two of the four measure the eval rather than the pipeline: `raw_replicate`
 * re-runs `raw` on a disjoint seed set (an A/A test, whose gap is the eval's
 * own drift), and `control` runs the pipeline with sampling deliberately broken
 * and must score far below `gglib`.
 */
export type EvalArm = 'raw' | 'gglib' | 'raw_replicate' | 'control';

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
}

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
  /**
   * Suite-wide wall clock, unfiltered — what running it cost. Never compare
   * two arms with it: a stalled run contributes its whole timeout.
   */
  total_wall_ms: number;
  /** Wall clock over the runs that reached the model — the comparable figure. */
  measured_wall_ms?: number;
  /**
   * Mean time to first tool call, over the tasks that made one.
   *
   * Read beside the median, never alone: the population is not unimodal, and a
   * handful of runs generating for a quarter of an hour drag this figure past
   * anything any individual run did.
   */
  mean_time_to_first_tool_call_ms?: number | null;
  /** Median time to first tool call — the typical run, which the mean stops describing. */
  median_time_to_first_tool_call_ms?: number | null;
  /**
   * What this arm generated, summed over its measured runs — except
   * `max_tool_calls_in_batch`, which is the arm-wide maximum, because one
   * runaway batch is the signal and a mean would dissolve it.
   */
  generated?: GeneratedOutput;
  /**
   * How many seeds every task was repeated under — the sample size behind every
   * mean above. Arms do not all share it: the control repeats fewer.
   */
  seeds?: number;
  /** Total task runs behind these scores: `tasks × seeds`. */
  runs?: number;
  /**
   * How many of those runs never reached the model, and so scored zero for it.
   *
   * Equal to `runs` is impossible in a delivered report — the eval aborts
   * rather than emit an arm with scores but no measurements. Between 1 and
   * `runs` means every mean on this arm is diluted by empty runs, and should be
   * read as a floor rather than a measurement.
   */
  unmeasured_runs?: number;
  /**
   * How many attempts this arm threw away to transport failures and retried.
   *
   * Independent of `unmeasured_runs` and worth showing on its own: an arm that
   * lost requests and won them back on a retry is fully measured and still not
   * clean, because its scores come from a later attempt than the suite
   * nominally ran. A report that showed only the survivors would call it
   * clean.
   */
  transport_retries?: number;
}

/**
 * Per-axis `gglib − raw` difference, plus the efficiency ratios.
 *
 * Quality axes are differences (positive means gglib scored higher);
 * efficiency figures are ratios `raw ÷ gglib` (above 1.0 means gglib did
 * better), because lower is better on those and the gaps are multiplicative.
 */
/**
 * Why an arm-level delta is not reported. A distinct state rather than a zero,
 * because "could not be taken" and "came out small" license different actions.
 */
export type DeltaWithheld = {
  kind: 'contaminated_by_unmeasured_runs';
  /** Unmeasured runs in the raw arm. */
  raw: number;
  /** Unmeasured runs in the gglib arm. */
  gglib: number;
};

export interface ArmDelta {
  /** `null` when `withheld` is set. */
  tool_accuracy?: number | null;
  /** `null` unless both arms measured the axis. */
  loop_avoidance?: number | null;
  /** `null` when `withheld` is set. */
  task_completion?: number | null;
  /**
   * Composite difference over the axes BOTH arms measured. `null` when
   * `withheld` is set.
   *
   * Not the difference of the two stored composites: each of those is
   * renormalized over whichever axes its own arm measured, so subtracting
   * across a mismatch measures the renormalization rather than the pipeline.
   */
  composite?: number | null;
  /** Why the axis differences above are absent, when they are. */
  withheld?: DeltaWithheld | null;
  /** Per-run wall-time speedup, `raw ÷ gglib`, over measured runs. */
  wall_time_speedup?: number | null;
  /** Per-run completion-token ratio, `raw ÷ gglib`, over measured runs. */
  completion_token_ratio?: number | null;
}

/**
 * One task's outcome under both arms.
 *
 * Each side carries **one entry per seed**, in seed order. A task that passes
 * 3/3 under one arm and 1/3 under the other is a different finding from 3/3
 * versus 0/3, and both collapse to "passed / failed" once the per-seed detail
 * is gone.
 */
export interface AgenticTaskComparison {
  task_id: string;
  category: TaskCategory;
  raw: TuneTaskResult[];
  gglib: TuneTaskResult[];
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
  /** The seeds every task ran under. Empty on a legacy or unseeded run. */
  seeds?: number[];
  /**
   * The positive control's scores, when it ran. What matters is not its value
   * but its distance below `gglib`; a control that failed to move invalidates
   * every delta in the report.
   */
  control?: ArmScores | null;
  /**
   * The A/A arm's scores — the raw pipeline again, on different seeds. Its
   * distance from `raw` is the eval's own drift, and the floor `delta` has to
   * clear before it describes a magnitude rather than a direction.
   */
  raw_replicate?: ArmScores | null;
  /** The seeds the A/A arm used. Empty when it did not run. */
  replicate_seeds?: number[];
  /**
   * Every A/A pair's scores when more than one ran. `raw_replicate` stays
   * populated with the first pair, so single-pair reports read unchanged;
   * the drift becomes the mean pairwise gap over all runs of the raw
   * configuration when this is non-empty.
   */
  raw_replicates?: ArmScores[];
  /** The seed set behind each entry of `raw_replicates`. */
  replicate_seed_sets?: number[][];
  /**
   * The paired per-(task, seed) comparison, computed server-side at report
   * assembly. Stored rather than re-derived here because it carries a rank
   * test (Wilcoxon signed-rank) nobody should maintain twice; `null` or
   * absent on reports written before the field existed.
   */
  paired?: PairedEffect | null;
}

/**
 * The paired view of raw-versus-gglib: every matched (task, seed) cell
 * compared directly, which removes the eval's identical-arm spread from the
 * comparison. Mirrors `gglib_core::domain::benchmark::agentic::PairedEffect`.
 */
export interface PairedEffect {
  /** Matched pairs in which both arms produced a real observation. */
  pairs: number;
  /** Pairs dropped because at least one side never reached the model. */
  unmeasured_pairs: number;
  /** Pairs the gglib arm scored strictly higher. */
  wins: number;
  /** Pairs the raw arm scored strictly higher. */
  losses: number;
  /** Pairs with identical scores. */
  ties: number;
  /** Mean of gglib − raw over the measured pairs. */
  mean_delta: number;
  /**
   * One-sided Wilcoxon signed-rank p for "gglib scores higher". `null` below
   * eight non-tied pairs — read wins against losses instead.
   */
  p_value: number | null;
}
