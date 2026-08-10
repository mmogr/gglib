/**
 * Agentic A/B eval mode: raw pipeline vs gglib pipeline, with validity arms.
 *
 * Mirrors TuneTab's streaming shape: the high-frequency
 * `agentic_task_complete` events are buffered and flushed into state every
 * 100 ms; coarse events (`agentic_arm_started`, `agentic_eval_complete`,
 * `run_failed`) set state directly. Aborting the stream genuinely cancels
 * the server-side run.
 *
 * @module components/Benchmark/Agentic/AgenticTab
 */

import { FC, useCallback, useEffect, useRef, useState } from 'react';
import { FlaskConical } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Banner } from '../../ui/Banner';
import { EmptyState } from '../../primitives';
import { AgenticConfigForm } from './AgenticConfigForm';
import { AgenticLiveProgress, type ArmProgress, type TaskLogEntry } from './AgenticLiveProgress';
import { AgenticReport } from './AgenticReport';
import { AgenticHistoryList } from './AgenticHistoryList';
import type { GgufModel } from '../../../types';
import type { AgenticEvalConfig, AgenticEvalReport, BenchmarkEvent } from '../../../types/benchmark';
import { startAgenticRun } from '../../../services/clients/benchmark';

interface AgenticRunState {
  status: 'idle' | 'running' | 'complete' | 'failed';
  arms: ArmProgress[];
  currentArm: ArmProgress['arm'] | null;
  taskLog: TaskLogEntry[];
  report: AgenticEvalReport | null;
  error?: string;
}

const IDLE: AgenticRunState = { status: 'idle', arms: [], currentArm: null, taskLog: [], report: null };

interface AgenticTabProps {
  models: GgufModel[];
  /** Called when a run finishes, so the page can refresh the shared history. */
  onRunComplete: () => void;
}

export const AgenticTab: FC<AgenticTabProps> = ({ models, onRunComplete }) => {
  const [runState, setRunState] = useState<AgenticRunState>(IDLE);
  const [historyReport, setHistoryReport] = useState<AgenticEvalReport | null>(null);

  const abortRef = useRef<AbortController | null>(null);
  const logBufferRef = useRef<TaskLogEntry[]>([]);
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
      const entries = logBufferRef.current;
      logBufferRef.current = [];
      if (entries.length === 0) return;
      setRunState((prev) => {
        const arms = prev.arms.map((a) => {
          const mine = entries.filter((e) => e.arm === a.arm);
          if (mine.length === 0) return a;
          return {
            ...a,
            done: a.done + mine.length,
            passed: a.passed + mine.filter((e) => e.passed).length,
          };
        });
        return { ...prev, arms, taskLog: [...prev.taskLog, ...entries] };
      });
    }, 100);
  }, []);

  const handleEvent = useCallback(
    (event: BenchmarkEvent) => {
      switch (event.type) {
        case 'agentic_arm_started':
          setRunState((prev) => ({
            ...prev,
            currentArm: event.arm,
            arms: [...prev.arms, { arm: event.arm, total: event.total_tasks, done: 0, passed: 0 }],
          }));
          break;

        case 'agentic_task_complete':
          logBufferRef.current.push({ arm: event.arm, taskId: event.task_id, passed: event.passed });
          scheduleFlush();
          break;

        case 'agentic_eval_complete':
          setRunState((prev) => ({ ...prev, status: 'complete', report: event.report }));
          onRunComplete();
          break;

        case 'run_failed':
          setRunState((prev) => ({ ...prev, status: 'failed', error: event.error }));
          break;
      }
    },
    [scheduleFlush, onRunComplete],
  );

  const handleSubmit = useCallback(
    (config: AgenticEvalConfig) => {
      abortRef.current?.abort();
      const abort = new AbortController();
      abortRef.current = abort;
      logBufferRef.current = [];
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      setHistoryReport(null);
      setRunState({ ...IDLE, status: 'running' });

      startAgenticRun(config, handleEvent, abort.signal)
        .then(() => {
          // The server can close the stream without any terminal event when a
          // run dies before its first checkpoint — surface that, don't hang.
          setRunState((prev) =>
            prev.status === 'running'
              ? { ...prev, status: 'failed', error: 'The eval stream ended without a report.' }
              : prev,
          );
        })
        .catch((err: Error) => {
          if (err.name !== 'AbortError') {
            setRunState((prev) => ({ ...prev, status: 'failed', error: err.message }));
          }
        });
    },
    [handleEvent],
  );

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
    setRunState(IDLE);
  }, []);

  const shownReport = runState.report ?? historyReport;

  return (
    <div className="flex flex-1 overflow-hidden gap-0">
      <aside className="w-[280px] shrink-0 flex flex-col gap-base p-base border-r border-border overflow-y-auto">
        <AgenticConfigForm
          models={models}
          isRunning={runState.status === 'running'}
          onSubmit={handleSubmit}
          onStop={handleStop}
        />
        {runState.status === 'failed' && runState.error && (
          <Banner variant="danger" title="Eval failed">
            {runState.error}
          </Banner>
        )}
      </aside>

      <div className="flex-1 overflow-y-auto p-base flex flex-col gap-base">
        {runState.status === 'idle' && !shownReport && (
          <EmptyState
            className="h-full"
            icon={<Icon icon={FlaskConical} size={24} />}
            title="No eval yet"
            description="Pick a model and press Run Eval to measure the gglib pipeline against the raw one — with a positive control and an A/A arm keeping the answer honest."
          />
        )}

        {runState.status === 'running' && runState.arms.length === 0 && (
          <p className="text-sm text-text-muted m-0">
            Loading the model and preparing arms — the first tasks appear here shortly…
          </p>
        )}

        {(runState.status === 'running' ||
          (runState.arms.length > 0 && runState.status !== 'idle')) && (
          <AgenticLiveProgress
            arms={runState.arms}
            currentArm={runState.status === 'running' ? runState.currentArm : null}
            taskLog={runState.taskLog}
          />
        )}

        {shownReport && <AgenticReport report={shownReport} />}

        {runState.status === 'idle' && (
          <AgenticHistoryList models={models} onSelect={setHistoryReport} />
        )}
      </div>
    </div>
  );
};
