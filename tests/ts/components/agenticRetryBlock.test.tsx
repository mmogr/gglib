/**
 * The retry banner in the agentic report's verdict stack.
 *
 * Its whole reason for existing is that a retried arm and a clean one are
 * fully-measured in exactly the same way — `unmeasured_runs` is zero on both —
 * so nothing else in the report tells them apart. If this block collapses the
 * two, the GUI reports a healthy run over an upstream that was failing
 * requests, which is the mistake the CLI's own retry line exists to prevent.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { AgenticReportVerdicts } from '../../../src/components/Benchmark/Agentic/AgenticReportVerdicts';
import type { AgenticEvalReport, ArmScores } from '../../../src/types/benchmark';

const arm = (overrides: Partial<ArmScores> = {}): ArmScores =>
  ({
    tool_accuracy: 0.9,
    loop_eligible: 2,
    task_completion: 0.9,
    composite: 0.9,
    total_wall_ms: 1000,
    runs: 63,
    unmeasured_runs: 0,
    transport_retries: 0,
    ...overrides,
  }) as ArmScores;

const report = (gglib: ArmScores): AgenticEvalReport =>
  ({
    model_name: 'Qwen3-4B',
    param_count_b: 4,
    ctx_size: 32768,
    raw: arm(),
    gglib,
    delta: { tool_accuracy: 0, task_completion: 0, composite: 0 },
    tasks: [],
  }) as AgenticEvalReport;

describe('AgenticReportVerdicts — transport retries', () => {
  it('reports an arm that was retried, even though every run was measured', () => {
    render(<AgenticReportVerdicts report={report(arm({ transport_retries: 3 }))} />);

    expect(screen.getByText(/Transport retries/i)).toBeInTheDocument();
    expect(screen.getByText(/gglib: 3 retried attempt\(s\) across 63 runs/)).toBeInTheDocument();
  });

  it('stays silent on a clean run, so the banner means something when it appears', () => {
    render(<AgenticReportVerdicts report={report(arm())} />);

    expect(screen.queryByText(/Transport retries/i)).not.toBeInTheDocument();
  });

  /**
   * The two states are independent: a run can be retried and recovered (fully
   * measured, not clean) or lost outright (unmeasured). Rendering one through
   * the other's banner would misreport both.
   */
  it('separates a recovered retry from a run that never reached the model', () => {
    render(
      <AgenticReportVerdicts
        report={report(arm({ transport_retries: 2, unmeasured_runs: 5 }))}
      />,
    );

    expect(screen.getByText(/2 retried attempt\(s\)/)).toBeInTheDocument();
    expect(screen.getByText(/5\/63 runs unmeasured/)).toBeInTheDocument();
  });
});
