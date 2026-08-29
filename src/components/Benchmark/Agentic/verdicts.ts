import type { AgenticEvalReport, AgenticTaskComparison } from '../../../types/benchmark';

/**
 * Client-side mirrors of `AgenticEvalReport`'s verdict methods
 * (`gglib_core::domain::benchmark::agentic`) — the Rust computes these as
 * methods, not serialized fields, so the GUI re-derives them. The thresholds
 * and edge cases must match the Rust exactly; the unit tests pin them.
 */

/**
 * A detection bar, not a quality bar: a control gap smaller than this means
 * the measurement did not respond to a change that should have been
 * impossible to miss. Mirrors `CONTROL_MIN_COMPOSITE_GAP`.
 */
export const CONTROL_MIN_COMPOSITE_GAP = 0.05;

/**
 * The smallest factor at which an effect and the eval's own drift are plainly
 * not the same size. A sanity ratio, not a significance test. Mirrors
 * `EFFECT_NOISE_RATIO`.
 */
export const EFFECT_NOISE_RATIO = 2.0;

export type ControlVerdict =
  | { kind: 'moved'; gap: number }
  | { kind: 'too_small'; gap: number }
  | { kind: 'wrong_direction'; gap: number };

export type EffectVerdict =
  | { kind: 'exceeds_noise'; effect: number; noise: number; pairs: number }
  | { kind: 'within_noise'; effect: number; noise: number; pairs: number };

/** What the positive control demonstrated, or `null` when it did not run. */
export function controlVerdict(report: AgenticEvalReport): ControlVerdict | null {
  const control = report.control;
  if (control == null) return null;
  const gap = report.gglib.composite - control.composite;
  if (gap >= CONTROL_MIN_COMPOSITE_GAP) return { kind: 'moved', gap };
  if (gap >= 0) return { kind: 'too_small', gap };
  return { kind: 'wrong_direction', gap: -gap };
}

/**
 * The eval's own drift: the mean pairwise composite gap over every run of the
 * identical raw configuration — the primary plus each A/A pair. With one pair
 * this is the old single gap; `null` when no A/A arm ran.
 */
export function noiseFloor(report: AgenticEvalReport): number | null {
  const gaps = driftGaps(report);
  if (gaps.length === 0) return null;
  return gaps.reduce((sum, gap) => sum + gap, 0) / gaps.length;
}

/** How many pairwise gaps stand behind the noise floor. */
export function noisePairs(report: AgenticEvalReport): number {
  return driftGaps(report).length;
}

function driftGaps(report: AgenticEvalReport): number[] {
  const composites = [report.raw.composite];
  const replicates = report.raw_replicates ?? [];
  if (replicates.length > 0) {
    composites.push(...replicates.map((r) => r.composite));
  } else if (report.raw_replicate != null) {
    composites.push(report.raw_replicate.composite);
  }
  const gaps: number[] = [];
  for (let i = 0; i < composites.length; i += 1) {
    for (let j = i + 1; j < composites.length; j += 1) {
      gaps.push(Math.abs(composites[i] - composites[j]));
    }
  }
  return gaps;
}

/**
 * Whether the measured effect is larger than the eval's own drift. `null`
 * when no A/A arm ran — then the composite delta is a direction, not a
 * magnitude. A zero effect never "exceeds" anything, however quiet the arm.
 *
 * Also `null` when the composite delta was withheld. Comparing a diluted
 * effect against a drift figure produces a confident ratio out of two numbers
 * that are not about the same thing, which is how a contaminated -0.058 came
 * to be reported as "8.3x the drift".
 */
export function effectVerdict(report: AgenticEvalReport): EffectVerdict | null {
  const noise = noiseFloor(report);
  if (noise == null) return null;
  const pairs = noisePairs(report);
  const effect = report.delta.composite;
  if (effect == null) return null;
  if (Math.abs(effect) > 0 && Math.abs(effect) >= EFFECT_NOISE_RATIO * noise) {
    return { kind: 'exceeds_noise', effect, noise, pairs };
  }
  return { kind: 'within_noise', effect, noise, pairs };
}

/** Per-arm pass counts for one task, in (raw, gglib) order. */
export function passCounts(task: AgenticTaskComparison): [number, number] {
  return [
    task.raw.filter((r) => r.passed).length,
    task.gglib.filter((r) => r.passed).length,
  ];
}

/** Whether either arm disagreed with itself across seeds. */
export function isUnstable(task: AgenticTaskComparison): boolean {
  const mixed = (runs: AgenticTaskComparison['raw']) =>
    runs.some((r) => r.passed) && runs.some((r) => !r.passed);
  return mixed(task.raw) || mixed(task.gglib);
}

/** Tasks whose outcome was not stable across seeds under either arm. */
export function unstableTasks(report: AgenticEvalReport): AgenticTaskComparison[] {
  return report.tasks.filter(isUnstable);
}
