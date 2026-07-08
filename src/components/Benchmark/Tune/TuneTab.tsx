/**
 * Tune-mode tab: orchestrates the config form, live progress, and
 * leaderboard sub-components. Owns all SSE/run state; the sub-components
 * are purely presentational.
 *
 * Throttling: mirrors the compare feature's pattern exactly — high-frequency
 * events (`tune_task_complete`, arriving once per task per candidate,
 * potentially in rapid bursts) are buffered in a `useRef` and flushed into
 * state every 100 ms. Coarser events (`tune_candidate_started`,
 * `tune_candidate_complete`, `tune_pruned`, `run_complete`/`run_failed`) —
 * the tune analog of compare's `model_started`/`model_complete` — update
 * state immediately, same as the compare feature.
 *
 * @module components/Benchmark/Tune/TuneTab
 */

import { FC, useCallback, useEffect, useRef, useState } from 'react';
import { AlertTriangle, Target } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import type { GgufModel } from '../../../types';
import type { BenchmarkEvent, TuneCandidateResult, TuneConfig } from '../../../types/benchmark';
import { startTuneRun } from '../../../services/clients/benchmark';
import { updateModel } from '../../../services/clients/models';
import { TuneConfigForm } from './TuneConfigForm';
import { TuneLiveProgress, TuneTaskLogEntry, TunePrunedEntry } from './TuneLiveProgress';
import { TuneLeaderboard } from './TuneLeaderboard';

interface TuneTabProps {
  models: GgufModel[];
}

interface TuneRunState {
  status: 'idle' | 'running' | 'complete' | 'failed';
  error?: string;
  total: number;
  currentCandidateIndex?: number;
  taskLog: TuneTaskLogEntry[];
  prunedLog: TunePrunedEntry[];
  results: TuneCandidateResult[];
}

const INITIAL_STATE: TuneRunState = {
  status: 'idle',
  total: 0,
  taskLog: [],
  prunedLog: [],
  results: [],
};

export const TuneTab: FC<TuneTabProps> = ({ models }) => {
  const [runState, setRunState] = useState<TuneRunState>(INITIAL_STATE);
  const [applyingIndex, setApplyingIndex] = useState<number | null>(null);
  const [applyMessage, setApplyMessage] = useState<string | null>(null);
  const [pendingModelId, setPendingModelId] = useState<number | null>(null);

  const abortRef = useRef<AbortController | null>(null);

  // Throttle buffer for tune_task_complete — same 100 ms pattern as
  // compare's model_text_delta buffering.
  const taskLogBufferRef = useRef<TuneTaskLogEntry[]>([]);
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
      const buffered = taskLogBufferRef.current;
      taskLogBufferRef.current = [];
      if (buffered.length === 0) return;
      setRunState(prev => ({ ...prev, taskLog: [...prev.taskLog, ...buffered] }));
    }, 100);
  }, []);

  const handleEvent = useCallback(
    (event: BenchmarkEvent) => {
      switch (event.type) {
        case 'tune_candidate_started':
          setRunState(prev => ({
            ...prev,
            total: event.total,
            currentCandidateIndex: event.candidate_index,
          }));
          break;

        case 'tune_task_complete':
          taskLogBufferRef.current.push({
            candidateIndex: event.candidate_index,
            taskId: event.task_id,
            passed: event.passed,
          });
          scheduleFlush();
          break;

        case 'tune_pruned':
          setRunState(prev => ({
            ...prev,
            prunedLog: [
              ...prev.prunedLog,
              { candidateIndex: event.candidate_index, reason: event.reason },
            ],
          }));
          break;

        case 'tune_candidate_complete':
          setRunState(prev => ({ ...prev, results: [...prev.results, event.result] }));
          break;

        case 'run_complete':
          setRunState(prev => ({ ...prev, status: 'complete' }));
          break;

        case 'run_failed':
          setRunState(prev => ({ ...prev, status: 'failed', error: event.error }));
          break;

        default:
          break;
      }
    },
    [scheduleFlush],
  );

  const handleApply = useCallback(async (result: TuneCandidateResult, modelId: number) => {
    setApplyMessage(null);
    try {
      await updateModel({ id: modelId, inferenceDefaults: result.config });
      setApplyMessage(
        `Applied config (score ${result.composite_score.toFixed(3)}) to the model's inference defaults.`,
      );
    } catch (err) {
      setApplyMessage(`Failed to apply config: ${(err as Error).message}`);
    }
  }, []);

  const handleSubmit = useCallback((config: TuneConfig, applyBest: boolean) => {
    abortRef.current?.abort();
    const abort = new AbortController();
    abortRef.current = abort;

    if (flushTimerRef.current !== null) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    taskLogBufferRef.current = [];
    setApplyMessage(null);
    setPendingModelId(config.model_id);
    setRunState({ ...INITIAL_STATE, status: 'running' });

    startTuneRun(config, handleEvent, abort.signal)
      .then(() => {
        setRunState(prev => {
          if (applyBest && prev.status !== 'failed') {
            const best = [...prev.results]
              .filter(r => !r.pruned)
              .sort((a, b) => b.composite_score - a.composite_score)[0];
            if (best) {
              void handleApply(best, config.model_id);
            }
          }
          return prev;
        });
      })
      .catch(err => {
        if ((err as Error).name !== 'AbortError') {
          setRunState(prev => ({ ...prev, status: 'failed', error: (err as Error).message }));
        }
      });
  }, [handleEvent, handleApply]);

  const isRunning = runState.status === 'running';

  return (
    <div className="flex flex-1 overflow-hidden gap-0">
      <aside className="w-[280px] shrink-0 flex flex-col gap-base p-base border-r border-border overflow-y-auto">
        <TuneConfigForm models={models} disabled={isRunning} onSubmit={handleSubmit} />
        {runState.status === 'failed' && runState.error && (
          <div className="text-xs text-danger bg-danger-subtle rounded-md p-sm flex items-center gap-xs">
            <Icon icon={AlertTriangle} size={14} />
            {runState.error}
          </div>
        )}
      </aside>

      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-y-auto p-base flex flex-col gap-base">
          {runState.status === 'idle' ? (
            <div className="flex flex-col items-center justify-center h-full text-text-muted gap-sm">
              <Icon icon={Target} size={32} className="opacity-30" />
              <p className="text-sm">
                Configure a sweep and press Run Tune to find the best sampling settings.
              </p>
            </div>
          ) : (
            <>
              <TuneLiveProgress
                total={runState.total}
                currentCandidateIndex={runState.currentCandidateIndex}
                taskLog={runState.taskLog}
                prunedLog={runState.prunedLog}
              />
              {applyMessage && (
                <div className="text-xs text-text bg-surface rounded-md p-sm">{applyMessage}</div>
              )}
              <div className="border border-border rounded-md overflow-x-auto">
                <TuneLeaderboard
                  results={runState.results}
                  applyingIndex={applyingIndex}
                  onApply={result => {
                    if (pendingModelId == null) return;
                    setApplyingIndex(runState.results.indexOf(result));
                    void handleApply(result, pendingModelId).finally(() =>
                      setApplyingIndex(null),
                    );
                  }}
                />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export default TuneTab;
