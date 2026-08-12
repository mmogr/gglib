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
import { EmptyState } from '../../primitives';
import { Icon } from '../../ui/Icon';
import type { GgufModel } from '../../../types';
import type {
  ApplyVerdict,
  BenchmarkEvent,
  TuneCandidateResult,
  TuneConfig,
} from '../../../types/benchmark';
import { applyTuneRun, startTuneRun } from '../../../services/clients/benchmark';
import { TuneConfigForm } from './TuneConfigForm';
import { TuneLiveProgress, TuneTaskLogEntry, TunePrunedEntry } from './TuneLiveProgress';
import { TuneLeaderboard } from './TuneLeaderboard';
import { getTransport } from '../../../services/transport';

interface TuneTabProps {
  models: GgufModel[];
  /** Called when a run finishes, so the page can refresh the shared history. */
  onRunComplete?: () => void;
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

/** One line per verdict, evidence included — refusals must read as refusals. */
function describeApplyVerdict(verdict: ApplyVerdict): string {
  switch (verdict.verdict) {
    case 'apply': {
      const paired = verdict.paired
        ? `; paired ${verdict.paired.wins}W-${verdict.paired.losses}L-${verdict.paired.ties}T`
        : '';
      return (
        `Applied as measured defaults: winner ${verdict.winner_composite.toFixed(3)} over ` +
        `incumbent ${verdict.incumbent_mean.toFixed(3)}, margin ` +
        `${verdict.margin >= 0 ? '+' : ''}${verdict.margin.toFixed(3)} against drift ` +
        `${verdict.drift.toFixed(3)}${paired}.`
      );
    }
    case 'incumbent_stands':
      return (
        `Incumbent stands at ${verdict.incumbent_mean.toFixed(3)}: no candidate beat the ` +
        `model's current defaults — the run's answer is "change nothing".`
      );
    case 'within_drift':
      return (
        `Not applied: the winner's ${verdict.margin >= 0 ? '+' : ''}` +
        `${verdict.margin.toFixed(3)} margin is inside the run's own ` +
        `${verdict.drift.toFixed(3)} drift — unresolved, not absent.`
      );
    case 'paired_disagrees':
      return (
        `Not applied: the winner's mean rests on a minority of tasks ` +
        `(${verdict.wins}W-${verdict.losses}L against the incumbent) — refused by the pairs.`
      );
    case 'uncalibrated':
      return 'Not applied: this run has no incumbent calibration pair; re-run the tune.';
    case 'contaminated':
      return (
        `Not applied: ${verdict.unmeasured_runs} task run(s) never reached the model, ` +
        `so the compared scores are contaminated.`
      );
    default:
      return 'Not applied: unrecognised verdict.';
  }
}

export const TuneTab: FC<TuneTabProps> = ({ models, onRunComplete }) => {
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
          completedRunIdRef.current = event.run_id;
          setRunState(prev => ({ ...prev, status: 'complete' }));
          onRunComplete?.();
          break;

        case 'run_failed':
          setRunState(prev => ({ ...prev, status: 'failed', error: event.error }));
          break;

        default:
          break;
      }
    },
    [scheduleFlush, onRunComplete],
  );

  const completedRunIdRef = useRef<number | null>(null);

  /**
   * The per-row apply: a person picked this exact candidate, which is a
   * deliberate choice — it writes as user-set defaults, the same as typing
   * the values into `gglib model update`. The gate governs the *automatic*
   * path below, not this one.
   */
  const handleApply = useCallback(async (result: TuneCandidateResult, modelId: number) => {
    setApplyMessage(null);
    try {
      await getTransport().updateModel({ id: modelId, inferenceDefaults: result.config });
      setApplyMessage(
        `Applied config (score ${result.composite_score.toFixed(3)}) to the model's inference defaults.`,
      );
    } catch (err) {
      setApplyMessage(`Failed to apply config: ${(err as Error).message}`);
    }
  }, []);

  /**
   * The automatic apply, through the gate: the daemon judges the stored run
   * (winner vs the incumbent calibration pair) and only an `apply` verdict
   * writes the model — as *measured* defaults. Every refusal renders with
   * its evidence.
   */
  const handleGatedApply = useCallback(async (runId: number) => {
    setApplyMessage(null);
    try {
      const outcome = await applyTuneRun(runId);
      setApplyMessage(describeApplyVerdict(outcome.verdict));
    } catch (err) {
      setApplyMessage(`Apply failed: ${(err as Error).message}`);
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
            const runId = completedRunIdRef.current;
            if (runId != null) {
              void handleGatedApply(runId);
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
  }, [handleEvent, handleGatedApply]);

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
            <EmptyState
              className="h-full"
              icon={<Icon icon={Target} size={24} />}
              title="No tune run yet"
              description="Configure a sweep and press Run Tune to score sampling candidates against the task suite."
            />
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
              <div className="bg-surface rounded-md overflow-x-auto">
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
