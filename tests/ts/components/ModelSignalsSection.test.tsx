/**
 * Tests for the proxy dashboard's per-model signals (#1092).
 *
 * Ported from the CLI's, which are the specification:
 * `render_defects_tests.rs` and `render_defects_agent_tests.rs`. Most assert
 * an absence — a row that must not show, a model that must not earn a card,
 * a "none" the run has not earned.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ModelSignalsSection } from '../../../src/components/proxy';
import type { ModelDefectCounts } from '../../../src/services/transport/types/dashboard';
import { dashboardSnapshot, modelDefectCounts } from '../fixtures/dashboard';

/** A frame whose models have each served 100 proxy requests, unless told otherwise. */
const withSignals = (perModel: Record<string, Partial<ModelDefectCounts>>) =>
  dashboardSnapshot({
    per_model_defects: Object.fromEntries(
      Object.entries(perModel).map(([model, counts]) => [
        model,
        modelDefectCounts({ requests: 100, ...counts }),
      ]),
    ),
  });

/** Every counter that is a signal rather than a denominator or a part of one. */
const SIGNALS = [
  'loop_guard_trips',
  'agent_guard_trips',
  'repairs_attempted',
  'stream_errors',
  'truncated_generations',
  'empty_responses',
  'dialect_residue',
  'unvalidatable_schemas',
  'normalization_errors',
  'identical_result_repeats',
  'repeats_not_evaluated',
  'repeats_rescued',
] as const satisfies readonly (keyof ModelDefectCounts)[];

/**
 * One model's card, top to bottom, a row as `label: value` and a row nested
 * under another indented by two spaces — so a test pins what shows, in what
 * order, and that nothing else does.
 */
function rowsOf(model: string): string[] {
  const card = screen.getByRole('group', { name: model });
  return Array.from(card.children, (row) => {
    const parts = row.children.length > 0 ? Array.from(row.children) : [row];
    const indent = row.classList.contains('pl-md') ? '  ' : '';
    return indent + parts.map((part) => part.textContent).join(': ');
  });
}

describe('ModelSignalsSection', () => {
  it('shows each model with something to report as the CLI prints it, and no other', () => {
    render(
      <ModelSignalsSection
        snapshot={withSignals({
          qwen: {
            repairs_attempted: 9,
            repairs_succeeded: 7,
            loop_guard_trips: 3,
            loop_guard_loops: 1,
            loop_guard_stagnations: 2,
            empty_responses: 4,
            reasoning_only: 3,
            identical_result_repeats: 2,
          },
          // Reached only through GUI chat: no proxy request, so its agent
          // trips are its whole record, read against their own decisions.
          llama: {
            requests: 0,
            agent_guard_scanned: 40,
            agent_guard_trips: 3,
            agent_guard_loops: 3,
            stream_errors: 5,
            repeats_rescued: 41,
          },
          healthy: {},
        })}
      />,
    );

    expect(screen.getByRole('heading', { name: 'Per-Model Signals' })).toBeInTheDocument();
    expect(rowsOf('qwen')).toEqual([
      'qwen: 100 requests',
      'Tool-call repairs: 7 of 9 succeeded',
      'Loop-guard trips: 3',
      '  Loop detector: 1',
      '  Stagnation detector: 2',
      'Empty responses: 4 (3 reasoning-only)',
      'Observed',
      '  Repeated, same result: 2',
    ]);
    expect(rowsOf('llama')).toEqual([
      'llama: No proxy requests',
      'Agent-path guard trips: 3 of 40 decisions',
      '  Loop detector: 3',
      'Stream errors: 5',
      'Observed',
      '  Repeated, new result: 41',
    ]);
    // In name order, as the CLI prints them, not the order the frame sent.
    expect(screen.getAllByRole('group').map((card) => card.getAttribute('aria-label'))).toEqual([
      'llama',
      'qwen',
    ]);
    // A clean model would bury the ones that are not.
    expect(screen.queryByRole('group', { name: 'healthy' })).not.toBeInTheDocument();
    expect(screen.queryByText(/none across/i)).not.toBeInTheDocument();
  });

  /**
   * Any one signal earns its model a card, the three repeat counters
   * included: a model whose one fault is a single stream error is not clean,
   * and "none" over it would be false. The CLI pins this for a few counters
   * alone; this pins it for each of the twelve.
   */
  it.each(SIGNALS)('gives a model whose only signal is %s a card', (signal) => {
    const counts: Partial<ModelDefectCounts> = {};
    counts[signal] = 1;
    render(<ModelSignalsSection snapshot={withSignals({ qwen: counts })} />);

    expect(screen.getByRole('group', { name: 'qwen' })).toBeInTheDocument();
    expect(screen.queryByText(/none across/i)).not.toBeInTheDocument();
  });

  /** Empty responses with no reasoning-only turn among them show the count alone. */
  it('shows empty responses without a reasoning-only part when there was none', () => {
    render(<ModelSignalsSection snapshot={withSignals({ qwen: { empty_responses: 1 } })} />);

    expect(rowsOf('qwen')).toEqual(['qwen: 100 requests', 'Empty responses: 1']);
  });

  /**
   * The header names the proxy's requests whenever the model had any, agent
   * turns or not, and says "No proxy requests" only for a model the agent
   * path alone reached; with neither, it shows the zero. A card with no
   * repeat counter fired has no "Observed" heading.
   */
  it('heads a card with its proxy requests, and shows no Observed heading without a repeat', () => {
    render(
      <ModelSignalsSection
        snapshot={withSignals({
          qwen: { agent_guard_scanned: 30, agent_guard_trips: 2, agent_guard_stagnations: 2 },
          llama: { requests: 0, stream_errors: 1 },
        })}
      />,
    );

    expect(rowsOf('qwen')).toEqual([
      'qwen: 100 requests',
      'Agent-path guard trips: 2 of 30 decisions',
      '  Stagnation detector: 2',
    ]);
    expect(rowsOf('llama')).toEqual(['llama: 0 requests', 'Stream errors: 1']);
  });

  /**
   * Every other counter under its own label, only when it fired; and a proxy
   * that predates the detector split shows its trips as the sum alone.
   */
  it('shows the remaining counters by name and leaves out what never fired', () => {
    render(
      <ModelSignalsSection
        snapshot={withSignals({
          qwen: {
            loop_guard_trips: 2,
            truncated_generations: 1,
            dialect_residue: 2,
            unvalidatable_schemas: 3,
            normalization_errors: 4,
            repeats_not_evaluated: 9,
          },
        })}
      />,
    );

    expect(rowsOf('qwen')).toEqual([
      'qwen: 100 requests',
      'Loop-guard trips: 2',
      'Truncated at ceiling: 1',
      'Dialect residue: 2',
      'Unvalidatable schemas: 3',
      'Normalization errors: 4',
      'Observed',
      '  Repeated, not comparable: 9',
    ]);
  });

  /** Before anything is recorded, "none" is a claim the run has not earned. */
  it('says nothing is recorded yet when no model has been seen', () => {
    render(<ModelSignalsSection snapshot={dashboardSnapshot()} />);

    expect(screen.getByText('Nothing recorded yet.')).toBeInTheDocument();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();
  });

  /**
   * A clean run says so over its denominator. Agent turns the guard passed
   * are not a signal, but they are part of what the "none" is claimed over,
   * named beside the requests rather than added to them.
   */
  it('says a clean run is clean over the requests and agent turns it saw', () => {
    const { rerender } = render(
      <ModelSignalsSection snapshot={withSignals({ qwen: { agent_guard_scanned: 30 } })} />,
    );
    expect(screen.getByText('None across 100 requests and 30 agent turns, 1 model.')).toBeInTheDocument();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();

    rerender(<ModelSignalsSection snapshot={withSignals({ qwen: {}, llama: { requests: 1 } })} />);
    expect(screen.getByText('None across 101 requests, 2 models.')).toBeInTheDocument();
  });

  it('renders no section before the first snapshot arrives', () => {
    const { container } = render(<ModelSignalsSection snapshot={null} />);

    expect(container).toBeEmptyDOMElement();
  });
});
