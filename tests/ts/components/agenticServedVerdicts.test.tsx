/**
 * The report's verdicts are the daemon's: the control's three outcomes and
 * each task's pass counts and stability are shown as served, never judged
 * again here (`with_verdicts` in Rust is what judges them).
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { AgenticReportVerdicts } from '../../../src/places/diagnostics/benchmarks/Agentic/AgenticReportVerdicts';
import type { AgenticEvalReport, AgenticTaskComparison, ControlVerdict } from '../../../src/types/benchmark';
import { agenticReport, armScores, taskResult } from '../fixtures/agentic';

/** A report whose control ran, with the verdict the daemon served for it. */
const withControl = (verdict: ControlVerdict): AgenticEvalReport =>
  agenticReport({
    gglib: armScores({ composite: 0.8 }),
    control: armScores({ composite: 0.7 }),
    control_verdict: verdict,
  });

describe('AgenticReportVerdicts — served verdicts', () => {
  it.each([
    [{ verdict: 'moved', gap: 0.1 } as const, /Control moved — the eval detects broken sampling/],
    [{ verdict: 'too_small', gap: 0.01 } as const, /Control barely moved/],
    [{ verdict: 'wrong_direction', gap: 0.02 } as const, /Control moved the wrong way/],
  ])('words the control verdict %o as served', (verdict, words) => {
    render(<AgenticReportVerdicts report={withControl(verdict)} />);
    expect(screen.getByText(words)).toBeInTheDocument();
  });

  it('says no control ran when none is served', () => {
    render(<AgenticReportVerdicts report={agenticReport()} />);
    expect(screen.getByText(/No control arm ran/)).toBeInTheDocument();
  });

  it('names the tasks served as unstable, with their served pass counts', () => {
    const task = (task_id: string, unstable: boolean): AgenticTaskComparison => ({
      task_id,
      category: 'single_call',
      raw: [taskResult(), taskResult({ passed: false })],
      gglib: [taskResult(), taskResult()],
      raw_passed: 1,
      gglib_passed: 2,
      unstable,
    });
    const report = agenticReport({ seeds: [1, 2], tasks: [task('steady', false), task('flips', true)] });

    render(<AgenticReportVerdicts report={report} />);

    expect(screen.getByText('1 task flipped between seeds:')).toBeInTheDocument();
    expect(screen.getByText('flips — raw 1/2, gglib 2/2 seeds passed')).toBeInTheDocument();
    expect(screen.queryByText(/^steady/)).not.toBeInTheDocument();
  });
});
