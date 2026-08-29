/**
 * The generation-shape table, and the two readings it exists to separate.
 *
 * Before it, a run was knowable as a token total and a wall time — and on
 * 2026-08-29 those numbers were identical for two situations needing opposite
 * responses: a small reasoning model thinking at length (a question about the
 * sampling recipe) and a model failing to stop (a bug). The report could not
 * say which it had seen.
 *
 * So what these tests hold is that the two render *differently*, and that the
 * ambiguous case is labelled as ambiguous rather than read as a finding.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { AgenticReport } from '../../../src/components/Benchmark/Agentic/AgenticReport';
import type { AgenticEvalReport, ArmScores, GeneratedOutput } from '../../../src/types/benchmark';

const arm = (generated: Partial<GeneratedOutput>, overrides: Partial<ArmScores> = {}): ArmScores =>
  ({
    tool_accuracy: 0.9,
    loop_eligible: 2,
    loop_avoidance: 1.0,
    task_completion: 0.9,
    composite: 0.9,
    total_wall_ms: 950_000,
    measured_wall_ms: 950_000,
    mean_time_to_first_tool_call_ms: 94_000,
    median_time_to_first_tool_call_ms: 1_029,
    total_completion_tokens: 32_986,
    runs: 63,
    unmeasured_runs: 0,
    transport_retries: 0,
    generated: {
      reasoning_chars: 0,
      answer_chars: 0,
      llm_calls: 3,
      max_tool_calls_in_batch: 1,
      system_warnings: 0,
      ...generated,
    },
    ...overrides,
  }) as ArmScores;

const report = (gglib: ArmScores): AgenticEvalReport =>
  ({
    model_name: 'Qwen3-4B',
    param_count_b: 4,
    ctx_size: 32768,
    seeds: [1, 2, 3],
    raw: arm({ reasoning_chars: 100, answer_chars: 400 }),
    gglib,
    delta: {},
    tasks: [],
  }) as AgenticEvalReport;

describe('AgenticReport — what was generated', () => {
  it('separates a thinking run from a repeating one', () => {
    const thinking = render(
      <AgenticReport report={report(arm({ reasoning_chars: 131_000, answer_chars: 400 }))} />,
    );
    expect(screen.getByText('131,000')).toBeInTheDocument();
    thinking.unmount();

    // Same tokens, same latency, same score — and it must not read the same.
    render(
      <AgenticReport
        report={report(arm({ answer_chars: 131_400, max_tool_calls_in_batch: 512 }))}
      />,
    );
    expect(screen.queryByText('131,000')).not.toBeInTheDocument();
    expect(screen.getByText('131,400')).toBeInTheDocument();
    expect(screen.getByText('512')).toBeInTheDocument();
  });

  it('calls zero reasoning ambiguous rather than letting it read as a finding', () => {
    render(<AgenticReport report={report(arm({ answer_chars: 131_400 }))} />);
    expect(screen.getByText(/Zero reasoning characters is ambiguous/i)).toBeInTheDocument();
  });

  it('says characters rather than letting a reader assume tokens', () => {
    render(<AgenticReport report={report(arm({ reasoning_chars: 500 }))} />);
    expect(screen.getByText(/Characters, not tokens/i)).toBeInTheDocument();
  });

  it('surfaces over-wide batch recoveries, which cost a request each', () => {
    render(<AgenticReport report={report(arm({ reasoning_chars: 10, system_warnings: 4 }))} />);
    expect(screen.getByText(/recovered from 4 over-wide tool-call/i)).toBeInTheDocument();
  });

  /**
   * A report written before any of this was recorded must say nothing, not
   * render a table of zeros that reads as "the model generated nothing".
   */
  it('stays silent on a report from before this existed', () => {
    const legacy = report(arm({}, { generated: undefined }));
    (legacy.raw as { generated?: GeneratedOutput }).generated = undefined;

    render(<AgenticReport report={legacy} />);
    expect(screen.queryByText('What was generated')).not.toBeInTheDocument();
  });

  /**
   * The metric that flipped between runs. Reporting the mean alone turned a
   * 1-second typical case into a 94-second headline; reporting the median alone
   * would hide the five runs entirely. The pair is what makes the spread visible.
   */
  it('shows median and mean first-call side by side', () => {
    render(<AgenticReport report={report(arm({ reasoning_chars: 10 }))} />);
    expect(screen.getByText('1st call (median)')).toBeInTheDocument();
    expect(screen.getByText('1st call (mean)')).toBeInTheDocument();
  });
});
