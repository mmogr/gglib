/**
 * Run state + SSE streaming for the perf/compare benchmark modes.
 *
 * Owns the abort controller and the 100 ms `model_text_delta` throttle
 * buffer; the tab component stays presentational.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { GgufModel } from '../../types';
import type { BenchmarkEvent, CompareConfig, PerfConfig } from '../../types/benchmark';
import { startCompareRun, startPerfRun } from '../../services/clients/benchmark';
import type { ModelResultState } from './PerfCompareResultCard';

export interface PerfCompareRunState {
  runId?: number;
  status: 'idle' | 'running' | 'complete' | 'failed';
  error?: string;
  models: ModelResultState[];
}

export function usePerfCompareRun(models: GgufModel[], onRunComplete: () => void) {
  const [runState, setRunState] = useState<PerfCompareRunState>({ status: 'idle', models: [] });

  const abortRef = useRef<AbortController | null>(null);
  const textBufferRef = useRef<Map<number, string>>(new Map());
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
      if (flushTimerRef.current !== null) clearTimeout(flushTimerRef.current);
    };
  }, []);

  const scheduleFlush = useCallback(() => {
    if (flushTimerRef.current !== null) return;
    flushTimerRef.current = setTimeout(() => {
      flushTimerRef.current = null;
      const snapshot = new Map(textBufferRef.current);
      textBufferRef.current = new Map();
      if (snapshot.size === 0) return;
      setRunState((prev) => ({
        ...prev,
        models: prev.models.map((m) => {
          const buf = snapshot.get(m.modelId);
          return buf != null ? { ...m, liveText: m.liveText + buf } : m;
        }),
      }));
    }, 100);
  }, []);

  const handleEvent = useCallback(
    (event: BenchmarkEvent) => {
      switch (event.type) {
        case 'model_started':
          setRunState((prev) => ({
            ...prev,
            models: prev.models.map((m) =>
              m.modelId === event.model_id ? { ...m, status: 'running' } : m,
            ),
          }));
          break;

        case 'model_text_delta':
          textBufferRef.current.set(
            event.model_id,
            (textBufferRef.current.get(event.model_id) ?? '') + event.text,
          );
          scheduleFlush();
          break;

        case 'model_complete':
          setRunState((prev) => ({
            ...prev,
            models: prev.models.map((m) =>
              m.modelId === event.model_id ? { ...m, status: 'complete', result: event.result } : m,
            ),
          }));
          break;

        case 'model_failed':
          setRunState((prev) => ({
            ...prev,
            models: prev.models.map((m) =>
              m.modelId === event.model_id ? { ...m, status: 'failed', error: event.error } : m,
            ),
          }));
          break;

        case 'run_complete':
          setRunState((prev) => ({ ...prev, status: 'complete', runId: event.run_id }));
          onRunComplete();
          break;

        case 'run_failed':
          setRunState((prev) => ({ ...prev, status: 'failed', error: event.error }));
          break;
      }
    },
    [scheduleFlush, onRunComplete],
  );

  const start = useCallback(
    async (config: CompareConfig | PerfConfig, kind: 'compare' | 'perf', modelIds: number[]) => {
      abortRef.current?.abort();
      const abort = new AbortController();
      abortRef.current = abort;

      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
        textBufferRef.current = new Map();
      }

      const modelStates: ModelResultState[] = modelIds.map((id) => {
        const m = models.find((x) => x.id === id);
        return { modelId: id, modelName: m?.name ?? `Model ${id}`, status: 'pending', liveText: '' };
      });
      setRunState({ status: 'running', models: modelStates });

      try {
        if (kind === 'compare') {
          await startCompareRun(config as CompareConfig, handleEvent, abort.signal);
        } else {
          await startPerfRun(config as PerfConfig, handleEvent, abort.signal);
        }
        // A stream that closes with no terminal event must not hang the UI.
        setRunState((prev) =>
          prev.status === 'running'
            ? { ...prev, status: 'failed', error: 'The benchmark stream ended without completing.' }
            : prev,
        );
      } catch (err) {
        if ((err as Error).name !== 'AbortError') {
          setRunState((prev) => ({ ...prev, status: 'failed', error: (err as Error).message }));
        }
      }
    },
    [models, handleEvent],
  );

  const stop = useCallback(() => {
    abortRef.current?.abort();
    setRunState((prev) => ({ ...prev, status: 'idle' }));
  }, []);

  return { runState, start, stop };
}
