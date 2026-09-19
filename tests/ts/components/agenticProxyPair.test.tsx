/**
 * The proxy pair's section of the agentic report.
 *
 * It mirrors the CLI's "through gglib-proxy" block. The repair counts are the
 * part that matters most: a repaired call reaches the agent as the repaired
 * call, so the scores cannot say whether repair ran, and a pair in which
 * nothing needed repair must say so rather than read as evidence either way.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { AgenticProxyPair } from '../../../src/components/Benchmark/Agentic/AgenticProxyPair';
import type { AgenticEvalReport, ArmScores, ProxyArms } from '../../../src/types/benchmark';
import type { ModelDefectCounts } from '../../../src/types/generated/ModelDefectCounts';

const arm = (composite: number): ArmScores =>
  ({
    tool_accuracy: composite,
    loop_eligible: 0,
    task_completion: composite,
    composite,
    total_wall_ms: 1000,
    runs: 18,
    unmeasured_runs: 0,
    transport_retries: 0,
  }) as ArmScores;

const counts = (overrides: Partial<ModelDefectCounts> = {}): ModelDefectCounts => ({
  requests: 24,
  loop_guard_trips: 0,
  loop_guard_loops: 0,
  loop_guard_stagnations: 0,
  repairs_attempted: 5,
  repairs_succeeded: 4,
  stream_errors: 0,
  truncated_generations: 0,
  empty_responses: 0,
  reasoning_only: 0,
  dialect_residue: 0,
  unvalidatable_schemas: 0,
  normalization_errors: 0,
  identical_result_repeats: 0,
  repeats_not_evaluated: 0,
  repeats_rescued: 0,
  ...overrides,
});

const report = (proxy?: Partial<ProxyArms>): AgenticEvalReport =>
  ({
    model_name: 'Llama-3.2-3B',
    param_count_b: 3,
    ctx_size: 8192,
    raw: arm(0.5),
    gglib: arm(0.6),
    delta: { tool_accuracy: 0.1, task_completion: 0.1, composite: 0.1 },
    tasks: [],
    proxy:
      proxy === undefined
        ? undefined
        : {
            raw_auto: arm(0.4),
            proxy: arm(0.9),
            delta: { tool_accuracy: 0.5, task_completion: 0.5, composite: 0.5 },
            paired: { pairs: 18, unmeasured_pairs: 0, wins: 9, losses: 1, ties: 8, mean_delta: 0.4, p_value: 0.004 },
            defects: counts(),
            settings: { tool_call_repair: true, loop_guard_mode: 'note' },
            tasks: [],
            ...proxy,
          },
  }) as AgenticEvalReport;

describe('AgenticProxyPair', () => {
  it('renders nothing for a report without the proxy pair', () => {
    const { container } = render(<AgenticProxyPair report={report()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows the pair and what the proxy repaired', () => {
    render(<AgenticProxyPair report={report({})} />);
    expect(screen.getByText('Through gglib-proxy')).toBeInTheDocument();
    // Three axes carry a delta in the fixture; loop avoidance has none.
    expect(screen.getAllByText('+0.500')).toHaveLength(3);
    expect(screen.getByText(/5 repair\(s\)\s+attempted, 4 succeeded/)).toBeInTheDocument();
    expect(screen.getByText(/the proxy scored higher on 9, raw \(auto\) on 1/)).toBeInTheDocument();
  });

  it('says so when the proxy attempted no repair, rather than reading as evidence', () => {
    const none = counts({ repairs_attempted: 0, repairs_succeeded: 0 });
    render(<AgenticProxyPair report={report({ defects: none })} />);
    expect(screen.getByText(/The proxy attempted no repair/)).toBeInTheDocument();
  });

  it('says so when repair was switched off', () => {
    render(
      <AgenticProxyPair
        report={report({ settings: { tool_call_repair: false, loop_guard_mode: 'note' } })}
      />,
    );
    expect(screen.getByText(/Repair was off/)).toBeInTheDocument();
    expect(screen.queryByText(/The proxy attempted no repair/)).not.toBeInTheDocument();
  });

  it('names the unmeasured runs when the delta is withheld', () => {
    const withheld = {
      tool_accuracy: null,
      task_completion: null,
      composite: null,
      withheld: { kind: 'contaminated_by_unmeasured_runs' as const, raw: 2, gglib: 3 },
    };
    render(<AgenticProxyPair report={report({ delta: withheld })} />);
    expect(
      screen.getByText(/Delta withheld: 2 raw \(auto\) and 3 proxy runs never reached/),
    ).toBeInTheDocument();
  });
});
