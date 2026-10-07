/**
 * A benchmark run that was aborted is not a run that failed.
 *
 * The three benchmark screens (perf/compare, tune, agentic eval) each abort
 * the request of a run that is stopped or superseded, and each used to tell
 * that abort from a failure with its own inline check on the error's name.
 * They ask `isAbortError` now. Nothing here is mocked below the screen but
 * `fetch`, which rejects as a real one does when its signal fires, so what
 * is recognised is the abort a browser makes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';

import type { AgenticEvalConfig, CompareConfig, TuneConfig } from '../../../src/types/benchmark';

vi.mock('../../../src/components/Benchmark/Tune/TuneConfigForm', () => ({
  // The real form is disabled while a run is going; this one can start a
  // second run over the first, which is what aborts the first.
  TuneConfigForm: ({ onSubmit }: { onSubmit: (config: TuneConfig, applyBest: boolean) => void }) => (
    <button type="button" onClick={() => onSubmit({ model_id: 1 } as TuneConfig, false)}>
      run tune
    </button>
  ),
}));
vi.mock('../../../src/components/Benchmark/Agentic/AgenticConfigForm', () => ({
  AgenticConfigForm: ({ onSubmit, onStop }: { onSubmit: (config: AgenticEvalConfig) => void; onStop: () => void }) => (
    <>
      <button type="button" onClick={() => onSubmit({ model_id: 1 } as AgenticEvalConfig)}>
        run eval
      </button>
      <button type="button" onClick={onStop}>
        stop eval
      </button>
    </>
  ),
}));
vi.mock('../../../src/components/Benchmark/Agentic/AgenticHistoryList', () => ({
  AgenticHistoryList: () => null,
}));

import { AgenticTab } from '../../../src/components/Benchmark/Agentic/AgenticTab';
import { TuneTab } from '../../../src/components/Benchmark/Tune/TuneTab';
import { usePerfCompareRun } from '../../../src/components/Benchmark/usePerfCompareRun';
import { setApiSession } from '../../../src/services/transport/api/client';

/** The signal of each request made, in order. */
let signals: AbortSignal[];
let fetchMock: ReturnType<typeof vi.fn>;

/** A request that never answers, and rejects as `fetch` does once its signal fires. */
function hang(_url: string, init: RequestInit): Promise<Response> {
  signals.push(init.signal!);
  return new Promise((_resolve, reject) => {
    init.signal!.addEventListener('abort', () => reject(init.signal!.reason));
  });
}

/** A request the daemon refuses, in its own words. */
async function refuse(_url: string, init: RequestInit): Promise<Response> {
  signals.push(init.signal!);
  return new Response(JSON.stringify({ error: 'model 1 is not loaded', status: 409, type: 'model_not_loaded' }), {
    status: 409,
    headers: { 'content-type': 'application/json' },
  });
}

/** Let a rejected request reach its handler. */
const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

beforeEach(() => {
  signals = [];
  fetchMock = vi.fn(hang);
  vi.stubGlobal('fetch', fetchMock);
  setApiSession('', undefined);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the perf and compare runs', () => {
  const config = { model_ids: [1] } as unknown as CompareConfig;

  it.each(['compare', 'perf'] as const)('a %s run aborted by the next one does not mark that one failed', async (kind) => {
    const { result, unmount } = renderHook(() => usePerfCompareRun([], vi.fn()));

    act(() => void result.current.start(config, kind, [1]));
    await waitFor(() => expect(signals).toHaveLength(1));
    act(() => void result.current.start(config, kind, [1]));
    await waitFor(() => expect(signals).toHaveLength(2));
    await settle();

    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
    expect(result.current.runState.status).toBe('running');
    expect(result.current.runState.error).toBeUndefined();
    unmount();
  });

  it('a stopped run goes back to idle and stays there', async () => {
    const { result, unmount } = renderHook(() => usePerfCompareRun([], vi.fn()));

    act(() => void result.current.start(config, 'compare', [1]));
    await waitFor(() => expect(signals).toHaveLength(1));
    act(() => result.current.stop());
    await settle();

    expect(signals[0].aborted).toBe(true);
    expect(result.current.runState.status).toBe('idle');
    expect(result.current.runState.error).toBeUndefined();
    unmount();
  });

  it('a run the daemon refuses fails with the daemon\'s sentence', async () => {
    fetchMock.mockImplementation(refuse);
    const { result, unmount } = renderHook(() => usePerfCompareRun([], vi.fn()));

    act(() => void result.current.start(config, 'compare', [1]));

    await waitFor(() => expect(result.current.runState.status).toBe('failed'));
    expect(result.current.runState.error).toBe('model 1 is not loaded');
    unmount();
  });
});

describe('the tune run', () => {
  it('aborted by the next one does not mark that one failed', async () => {
    render(<TuneTab models={[]} />);

    fireEvent.click(screen.getByText('run tune'));
    await waitFor(() => expect(signals).toHaveLength(1));
    fireEvent.click(screen.getByText('run tune'));
    await waitFor(() => expect(signals).toHaveLength(2));
    await settle();

    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
    expect(screen.queryByText(/abort/i)).toBeNull();
    expect(document.querySelector('.text-danger')).toBeNull();
  });

  it('refused by the daemon fails with the daemon\'s sentence', async () => {
    fetchMock.mockImplementation(refuse);
    render(<TuneTab models={[]} />);

    fireEvent.click(screen.getByText('run tune'));

    expect(await screen.findByText('model 1 is not loaded')).toBeInTheDocument();
  });
});

describe('the agentic eval', () => {
  it('stopped by the person goes back to idle, with no failure shown', async () => {
    render(<AgenticTab models={[]} onRunComplete={vi.fn()} />);

    fireEvent.click(screen.getByText('run eval'));
    await waitFor(() => expect(signals).toHaveLength(1));
    expect(screen.getByText(/Loading the model and preparing arms/)).toBeInTheDocument();
    fireEvent.click(screen.getByText('stop eval'));
    await settle();

    expect(signals[0].aborted).toBe(true);
    expect(screen.queryByText('Eval failed')).toBeNull();
    expect(screen.getByText('No eval yet')).toBeInTheDocument();
  });

  it('refused by the daemon fails with the daemon\'s sentence', async () => {
    fetchMock.mockImplementation(refuse);
    render(<AgenticTab models={[]} onRunComplete={vi.fn()} />);

    fireEvent.click(screen.getByText('run eval'));

    expect(await screen.findByText('Eval failed')).toBeInTheDocument();
    expect(screen.getByText('model 1 is not loaded')).toBeInTheDocument();
  });
});
