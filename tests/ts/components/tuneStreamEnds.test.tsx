/**
 * A tune run whose stream closes without finishing is a run that failed.
 *
 * The daemon ends a tune run's stream with `run_complete` or `run_failed`. A
 * stream that closes with neither has nothing more to send, so the screen has
 * to stop waiting for it: the run is shown as failed and the form can be used
 * again. The perf/compare and agentic-eval screens already did this; the tune
 * screen stayed on its progress view with the form disabled.
 *
 * Nothing here is mocked below the screen but `fetch`, so what closes is a
 * response body, read by the reader the app reads every stream with.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import type { BenchmarkEvent, TuneConfig } from '../../../src/types/benchmark';

vi.mock('../../../src/components/Benchmark/Tune/TuneConfigForm', () => ({
  // The real form's fields are beside the point; what it shares with this one
  // is that it cannot start a run while `disabled`.
  TuneConfigForm: ({
    disabled,
    onSubmit,
  }: {
    disabled: boolean;
    onSubmit: (config: TuneConfig, applyBest: boolean) => void;
  }) => (
    <>
      <button type="button" disabled={disabled} onClick={() => onSubmit({ model_id: 1 } as TuneConfig, false)}>
        run tune
      </button>
      <button type="button" disabled={disabled} onClick={() => onSubmit({ model_id: 1 } as TuneConfig, true)}>
        run tune and apply
      </button>
    </>
  ),
}));

import { TuneTab } from '../../../src/components/Benchmark/Tune/TuneTab';
import { setApiSession } from '../../../src/services/transport/api/client';

const STREAM_ENDED = 'The tune stream ended without completing.';

const started: BenchmarkEvent = { type: 'tune_candidate_started', candidate_index: 0, total: 2 };
const completed = (runId: number): BenchmarkEvent => ({ type: 'run_complete', run_id: runId });

/** The events each tune run's stream sends before it closes, one entry per run, in order. */
let runs: BenchmarkEvent[][];
/** The path of each apply request made, in order. */
let applies: string[];

/** A tune stream that sends its events and closes; an apply that is refused by the gate. */
async function daemon(url: string): Promise<Response> {
  if (url.endsWith('/apply')) {
    applies.push(url);
    return new Response(JSON.stringify({ verdict: { verdict: 'uncalibrated' }, model_id: 1, applied: false }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  }
  const events = runs.shift() ?? [];
  const body = events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join('');
  return new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(body));
        controller.close();
      },
    }),
    { status: 200, headers: { 'content-type': 'text/event-stream' } },
  );
}

/** Let a stream that has closed, and anything it started, reach the screen. */
const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

beforeEach(() => {
  runs = [];
  applies = [];
  vi.stubGlobal('fetch', vi.fn(daemon));
  setApiSession('', undefined);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('a tune run whose stream closes', () => {
  it.each([
    ['before any event', []],
    ['after progress but no terminal event', [started]],
  ])('%s ends in an error, and the form can be used again', async (_when, events) => {
    runs = [events];
    render(<TuneTab models={[]} />);

    fireEvent.click(screen.getByText('run tune'));

    expect(await screen.findByText(STREAM_ENDED)).toBeInTheDocument();
    expect(screen.getByText('run tune')).toBeEnabled();
  });

  it('after `run_complete` is a finished run, not a failed one', async () => {
    runs = [[started, completed(7)]];
    render(<TuneTab models={[]} />);

    fireEvent.click(screen.getByText('run tune'));

    await waitFor(() => expect(screen.getByText('run tune')).toBeEnabled());
    await settle();
    expect(screen.queryByText(STREAM_ENDED)).toBeNull();
    expect(document.querySelector('.text-danger')).toBeNull();
  });

  it('after `run_failed` keeps the daemon\'s own sentence', async () => {
    runs = [[started, { type: 'run_failed', error: 'model 1 is not loaded' }]];
    render(<TuneTab models={[]} />);

    fireEvent.click(screen.getByText('run tune'));

    expect(await screen.findByText('model 1 is not loaded')).toBeInTheDocument();
    await settle();
    expect(screen.queryByText(STREAM_ENDED)).toBeNull();
  });

  it('without finishing applies nothing, even after an earlier run that finished', async () => {
    runs = [[started, completed(7)], [started]];
    render(<TuneTab models={[]} />);

    // The first run finishes, so its winner goes to the apply gate.
    fireEvent.click(screen.getByText('run tune and apply'));
    await waitFor(() => expect(applies).toContain('/api/benchmark/tune/7/apply'));
    await waitFor(() => expect(screen.getByText('run tune and apply')).toBeEnabled());
    await settle();
    const appliedByTheFirstRun = applies.length;

    // The second one does not: there is no run of its own to apply, and the
    // first run's id must not stand in for it.
    fireEvent.click(screen.getByText('run tune and apply'));

    expect(await screen.findByText(STREAM_ENDED)).toBeInTheDocument();
    await settle();
    expect(applies).toHaveLength(appliedByTheFirstRun);
  });
});
