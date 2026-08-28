/**
 * The withheld-delta banner, and the verdict that must go quiet with it.
 *
 * The failure this guards against is not a missing warning — the 2026-08-28
 * report printed one. It printed the contaminated -0.058 as its headline too,
 * and the number is what got read. So the test that matters is that the
 * *number* is gone, not that a caption appeared beside it.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { AgenticReportVerdicts } from '../../../src/components/Benchmark/Agentic/AgenticReportVerdicts';
import { effectVerdict } from '../../../src/components/Benchmark/Agentic/verdicts';
import type { AgenticEvalReport, ArmDelta, ArmScores } from '../../../src/types/benchmark';

const arm = (composite: number, overrides: Partial<ArmScores> = {}): ArmScores =>
  ({
    tool_accuracy: composite,
    loop_eligible: 2,
    loop_avoidance: 1.0,
    task_completion: composite,
    composite,
    total_wall_ms: 1000,
    measured_wall_ms: 1000,
    runs: 63,
    unmeasured_runs: 0,
    transport_retries: 0,
    ...overrides,
  }) as ArmScores;

const report = (delta: ArmDelta, gglib: ArmScores): AgenticEvalReport =>
  ({
    model_name: 'Qwen3-4B',
    param_count_b: 4,
    ctx_size: 32768,
    raw: arm(0.947),
    gglib,
    delta,
    tasks: [],
    // An A/A arm is present, so a verdict would be computable if the delta were.
    raw_replicate: arm(0.954),
    replicate_seeds: [1, 2, 3],
  }) as AgenticEvalReport;

describe('AgenticReportVerdicts — withheld delta', () => {
  const withheldDelta: ArmDelta = {
    tool_accuracy: null,
    loop_avoidance: null,
    task_completion: null,
    composite: null,
    withheld: { kind: 'contaminated_by_unmeasured_runs', raw: 0, gglib: 5 },
  };

  it('names the runs that never reached the model', () => {
    render(
      <AgenticReportVerdicts
        report={report(withheldDelta, arm(0.889, { unmeasured_runs: 5 }))}
      />,
    );

    expect(screen.getByText(/Delta withheld/i)).toBeInTheDocument();
    expect(
      screen.getByText(/0 raw and 5 gglib run\(s\) never reached the model/),
    ).toBeInTheDocument();
  });

  it('stays silent on a clean report', () => {
    const clean: ArmDelta = {
      tool_accuracy: -0.04,
      loop_avoidance: null,
      task_completion: -0.016,
      composite: -0.058,
      withheld: null,
    };
    render(<AgenticReportVerdicts report={report(clean, arm(0.889))} />);

    expect(screen.queryByText(/Delta withheld/i)).not.toBeInTheDocument();
  });

  /**
   * The drift ratio is built on the composite delta. Computing it from a
   * diluted effect yields a confident number about two things that are not the
   * same — which is how a contaminated -0.058 was reported as "8.3x the drift".
   */
  it('withholds the drift verdict along with the delta it is built on', () => {
    const contaminated = report(withheldDelta, arm(0.889, { unmeasured_runs: 5 }));
    expect(effectVerdict(contaminated)).toBeNull();

    // Same arms, same drift, delta present: the verdict is computable again,
    // which is what proves the guard above is doing the work.
    const measurable = report(
      { ...withheldDelta, composite: -0.058, withheld: null },
      arm(0.889),
    );
    expect(effectVerdict(measurable)).not.toBeNull();
  });
});
