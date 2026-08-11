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
  | { kind: 'exceeds_noise'; effect: number; noise: number }
  | { kind: 'within_noise'; effect: number; noise: number };

/** What the positive control demonstrated, or `null` when it did not run. */
export function controlVerdict(report: AgenticEvalReport): ControlVerdict | null {
  const control = report.control;
  if (control == null) return null;
  const gap = report.gglib.composite - control.composite;
  if (gap >= CONTROL_MIN_COMPOSITE_GAP) return { kind: 'moved', gap };
  if (gap >= 0) return { kind: 'too_small', gap };
  return { kind: 'wrong_direction', gap: -gap };
}

/** The eval's own drift: how far two identical raw arms landed apart. `null` when the A/A arm did not run. */
export function noiseFloor(report: AgenticEvalReport): number | null {
  const replicate = report.raw_replicate;
  if (replicate == null) return null;
  return Math.abs(report.raw.composite - replicate.composite);
}

/**
 * Whether the measured effect is larger than the eval's own drift. `null`
 * when no A/A arm ran — then the composite delta is a direction, not a
 * magnitude. A zero effect never "exceeds" anything, however quiet the arm.
 */
export function effectVerdict(report: AgenticEvalReport): EffectVerdict | null {
  const noise = noiseFloor(report);
  if (noise == null) return null;
  const effect = report.delta.composite;
  if (Math.abs(effect) > 0 && Math.abs(effect) >= EFFECT_NOISE_RATIO * noise) {
    return { kind: 'exceeds_noise', effect, noise };
  }
  return { kind: 'within_noise', effect, noise };
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
