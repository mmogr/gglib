/**
 * Tests for the client-side agentic verdict derivations.
 *
 * These mirror Rust methods on `AgenticEvalReport` (control_verdict,
 * noise_floor, effect_verdict, is_unstable) — the assertions pin the exact
 * thresholds and edge cases so the two implementations cannot drift.
 */

import { describe, it, expect } from 'vitest';
import {
  CONTROL_MIN_COMPOSITE_GAP,
  EFFECT_NOISE_RATIO,
  controlVerdict,
  effectVerdict,
  isUnstable,
  noiseFloor,
  passCounts,
  unstableTasks,
} from '../../../src/components/Benchmark/Agentic/verdicts';
import type {
  AgenticEvalReport,
  AgenticTaskComparison,
  ArmScores,
  TuneTaskResult,
} from '../../../src/types/benchmark';

function arm(composite: number): ArmScores {
  return {
    tool_accuracy: 0.5,
    loop_eligible: 1,
    task_completion: 0.5,
    composite,
    total_wall_ms: 1000,
  } as ArmScores;
}

function report(overrides: Partial<AgenticEvalReport>): AgenticEvalReport {
  return {
    model_name: 'm',
    param_count_b: 4,
    ctx_size: 8192,
    raw: arm(0.5),
    gglib: arm(0.6),
    delta: { tool_accuracy: 0, task_completion: 0, composite: 0.1 },
    tasks: [],
    ...overrides,
  } as AgenticEvalReport;
}

function run(passed: boolean): TuneTaskResult {
  return { passed } as TuneTaskResult;
}

function comparison(raw: boolean[], gglib: boolean[]): AgenticTaskComparison {
  return {
    task_id: 't',
    category: 'tool_call' as AgenticTaskComparison['category'],
    raw: raw.map(run),
    gglib: gglib.map(run),
  };
}

describe('controlVerdict', () => {
  it('returns null when no control arm ran', () => {
    expect(controlVerdict(report({ control: null }))).toBeNull();
    expect(controlVerdict(report({}))).toBeNull();
  });

  it('reports moved at or above the detection gap', () => {
    // Dyadic fixtures: 0.5 − 0.4375 = 0.0625 exactly, no float boundary fuzz.
    const v = controlVerdict(report({ gglib: arm(0.5), control: arm(0.4375) }));
    expect(v).toEqual({ kind: 'moved', gap: 0.0625 });
    expect(CONTROL_MIN_COMPOSITE_GAP).toBe(0.05);
  });

  it('reports too_small for a nonnegative gap under the bar', () => {
    const v = controlVerdict(report({ gglib: arm(0.5), control: arm(0.46875) }));
    expect(v?.kind).toBe('too_small');
  });

  it('reports wrong_direction with the gap made positive', () => {
    const v = controlVerdict(report({ gglib: arm(0.6), control: arm(0.7) }));
    expect(v).toEqual({ kind: 'wrong_direction', gap: expect.closeTo(0.1, 10) });
  });
});

describe('effectVerdict', () => {
  it('returns null when no A/A arm ran', () => {
    expect(effectVerdict(report({}))).toBeNull();
  });

  it('exceeds noise only at the 2.0x factor', () => {
    // Dyadic fixtures: noise = |0.5 − 0.53125| = 0.03125; 2× = 0.0625 exactly.
    const base = { raw: arm(0.5), raw_replicate: arm(0.53125) };
    expect(EFFECT_NOISE_RATIO).toBe(2.0);
    const exceeds = effectVerdict(
      report({ ...base, delta: { tool_accuracy: 0, task_completion: 0, composite: 0.0625 } }),
    );
    expect(exceeds?.kind).toBe('exceeds_noise');
    const within = effectVerdict(
      report({ ...base, delta: { tool_accuracy: 0, task_completion: 0, composite: 0.046875 } }),
    );
    expect(within?.kind).toBe('within_noise');
  });

  it('never lets a zero effect exceed a zero noise floor', () => {
    const v = effectVerdict(
      report({
        raw: arm(0.5),
        raw_replicate: arm(0.5),
        delta: { tool_accuracy: 0, task_completion: 0, composite: 0 },
      }),
    );
    expect(v?.kind).toBe('within_noise');
  });

  it('measures noise as the absolute raw-vs-replicate distance', () => {
    expect(noiseFloor(report({ raw: arm(0.5), raw_replicate: arm(0.45) }))).toBeCloseTo(0.05);
  });
});

describe('task stability', () => {
  it('counts each arm separately', () => {
    expect(passCounts(comparison([true, false, false], [true, true, true]))).toEqual([1, 3]);
  });

  it('flags a flip in either arm as unstable', () => {
    expect(isUnstable(comparison([true, false], [true, true]))).toBe(true);
    expect(isUnstable(comparison([true, true], [false, true]))).toBe(true);
    expect(isUnstable(comparison([true, true], [false, false]))).toBe(false);
  });

  it('collects unstable tasks from the report', () => {
    const stable = comparison([true], [true]);
    const unstable = comparison([true, false], [true, true]);
    expect(unstableTasks(report({ tasks: [stable, unstable] }))).toEqual([unstable]);
  });
});
