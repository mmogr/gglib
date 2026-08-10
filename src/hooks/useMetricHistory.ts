import { useEffect, useRef, useState } from 'react';

export type MetricHistoryMode = 'value' | 'rate';

interface UseMetricHistoryOptions {
  /** Ring-buffer size; oldest samples fall off. */
  capacity?: number;
  /**
   * 'value' pushes each sample as-is; 'rate' treats the sample as a cumulative
   * counter and pushes its per-second delta, measured against wall-clock time
   * (stream cadence is not guaranteed regular, so dt must be measured).
   */
  mode?: MetricHistoryMode;
  /** Changing this clears the buffer (e.g. the model/slot the series belongs to). */
  resetKey?: unknown;
  /**
   * Change-signal for when to record. Defaults to the sample itself, which
   * misses repeats — pass the snapshot's timestamp/sequence so an unchanged
   * counter still registers (a stalled counter is a zero rate, not no data).
   */
  tick?: unknown;
}

interface MetricHistory {
  values: number[];
  latest: number | null;
}

/**
 * Accumulates a bounded history of a live metric for Readout/Sparkline use.
 * Rate mode clamps negative deltas to zero — counters reset when a model is
 * swapped or a server restarts, and a reset is not a negative rate.
 */
const UNSET = Symbol('useMetricHistory.unset');

export function useMetricHistory(
  sample: number | null | undefined,
  { capacity = 60, mode = 'value', resetKey, tick }: UseMetricHistoryOptions = {},
): MetricHistory {
  const [values, setValues] = useState<number[]>([]);
  const prevCounterRef = useRef<{ value: number; at: number } | null>(null);
  const lastSignalRef = useRef<unknown>(UNSET);

  useEffect(() => {
    setValues([]);
    prevCounterRef.current = null;
    lastSignalRef.current = UNSET;
  }, [resetKey]);

  const changeSignal = tick === undefined ? sample : tick;

  useEffect(() => {
    if (sample == null || Number.isNaN(sample)) return;

    // Only the change-signal records — mode/capacity are in the deps for
    // correctness but must not act as record triggers.
    if (Object.is(lastSignalRef.current, changeSignal)) return;
    lastSignalRef.current = changeSignal;

    if (mode === 'value') {
      setValues(prev => [...prev, sample].slice(-capacity));
      return;
    }

    const now = Date.now();
    const prev = prevCounterRef.current;
    prevCounterRef.current = { value: sample, at: now };
    if (!prev) return;

    const dtSeconds = (now - prev.at) / 1000;
    if (dtSeconds <= 0) return;

    const rate = Math.max(0, (sample - prev.value) / dtSeconds);
    setValues(prevValues => [...prevValues, rate].slice(-capacity));
  }, [sample, changeSignal, mode, capacity]);

  return { values, latest: values.length > 0 ? values[values.length - 1] : null };
}
