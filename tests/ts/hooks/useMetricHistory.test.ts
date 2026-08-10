/**
 * Tests for useMetricHistory hook.
 *
 * Ring-buffer accumulation for Readout/Sparkline telemetry: value mode,
 * counter-derived rate mode with measured dt, negative-delta clamping,
 * capacity trimming, tick-driven repeats, and resetKey clearing.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useMetricHistory } from '../../../src/hooks/useMetricHistory';

describe('useMetricHistory', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('accumulates samples in value mode', () => {
    const { result, rerender } = renderHook(({ s }) => useMetricHistory(s), {
      initialProps: { s: 10 },
    });
    rerender({ s: 20 });
    rerender({ s: 30 });

    expect(result.current.values).toEqual([10, 20, 30]);
    expect(result.current.latest).toBe(30);
  });

  it('ignores null, undefined, and NaN samples', () => {
    const { result, rerender } = renderHook(
      ({ s }: { s: number | null | undefined }) => useMetricHistory(s),
      { initialProps: { s: 5 as number | null | undefined } },
    );
    rerender({ s: null });
    rerender({ s: undefined });
    rerender({ s: Number.NaN });

    expect(result.current.values).toEqual([5]);
  });

  it('trims to capacity, dropping the oldest samples', () => {
    const { result, rerender } = renderHook(
      ({ s }) => useMetricHistory(s, { capacity: 3 }),
      { initialProps: { s: 1 } },
    );
    for (const s of [2, 3, 4, 5]) rerender({ s });

    expect(result.current.values).toEqual([3, 4, 5]);
  });

  it('derives rates from cumulative counters using measured elapsed time', () => {
    const { result, rerender } = renderHook(
      ({ s }) => useMetricHistory(s, { mode: 'rate' }),
      { initialProps: { s: 100 } },
    );
    expect(result.current.values).toEqual([]);

    vi.advanceTimersByTime(2000);
    rerender({ s: 300 });

    expect(result.current.values).toEqual([100]);
    expect(result.current.latest).toBe(100);
  });

  it('clamps negative counter deltas to zero', () => {
    const { result, rerender } = renderHook(
      ({ s }) => useMetricHistory(s, { mode: 'rate' }),
      { initialProps: { s: 300 } },
    );
    vi.advanceTimersByTime(1000);
    rerender({ s: 50 });

    expect(result.current.values).toEqual([0]);
  });

  it('registers a zero rate when the counter stalls but the tick advances', () => {
    const { result, rerender } = renderHook(
      ({ s, t }) => useMetricHistory(s, { mode: 'rate', tick: t }),
      { initialProps: { s: 100, t: 1 } },
    );
    vi.advanceTimersByTime(1000);
    rerender({ s: 100, t: 2 });

    expect(result.current.values).toEqual([0]);
  });

  it('does not record when only options change', () => {
    const { result, rerender } = renderHook(
      ({ s, c }) => useMetricHistory(s, { capacity: c }),
      { initialProps: { s: 42, c: 60 } },
    );
    rerender({ s: 42, c: 59 });
    rerender({ s: 42, c: 58 });

    expect(result.current.values).toEqual([42]);
  });

  it('clears the buffer when resetKey changes', () => {
    const { result, rerender } = renderHook(
      ({ s, k }) => useMetricHistory(s, { resetKey: k }),
      { initialProps: { s: 10, k: 'model-a' } },
    );
    rerender({ s: 20, k: 'model-a' });
    expect(result.current.values).toEqual([10, 20]);

    rerender({ s: 20, k: 'model-b' });
    expect(result.current.values).toEqual([]);

    rerender({ s: 30, k: 'model-b' });
    expect(result.current.values).toEqual([30]);
  });
});
