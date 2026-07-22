/**
 * Live progress display for an in-flight tune run: current candidate
 * position, a scrolling log of per-task pass/fail results, and pruning
 * notices.
 *
 * Purely presentational — all throttling of the underlying SSE events
 * happens in the parent (`TuneTab`), matching the compare feature's
 * throttled-buffer pattern.
 *
 * @module components/Benchmark/Tune/TuneLiveProgress
 */

import { FC } from 'react';
import { Check, X } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';

export interface TuneTaskLogEntry {
  candidateIndex: number;
  taskId: string;
  passed: boolean;
}

export interface TunePrunedEntry {
  candidateIndex: number;
  reason: string;
}

interface TuneLiveProgressProps {
  total: number;
  currentCandidateIndex?: number;
  taskLog: TuneTaskLogEntry[];
  prunedLog: TunePrunedEntry[];
}

export const TuneLiveProgress: FC<TuneLiveProgressProps> = ({
  total,
  currentCandidateIndex,
  taskLog,
  prunedLog,
}) => {
  const progressPct =
    total > 0 && currentCandidateIndex != null
      ? Math.min(100, ((currentCandidateIndex + 1) / total) * 100)
      : 0;

  return (
    <div className="flex flex-col gap-sm">
      <div className="flex items-center gap-sm text-sm text-text-secondary">
        <span>
          Candidate {currentCandidateIndex != null ? currentCandidateIndex + 1 : 0} / {total}
        </span>
      </div>
      <div className="w-full h-2 bg-surface rounded-full overflow-hidden">
        <div
          className="h-full bg-primary transition-all duration-200"
          style={{ width: `${progressPct}%` }}
        />
      </div>

      <div className="flex flex-col gap-xs max-h-[220px] overflow-y-auto border border-border rounded-md p-sm">
        {taskLog.length === 0 && prunedLog.length === 0 && (
          <p className="text-xs text-text-muted">Waiting for events…</p>
        )}
        {taskLog.map((entry, i) => (
          <div
            key={`${entry.candidateIndex}-${entry.taskId}-${i}`}
            className="flex items-center gap-sm text-xs"
          >
            <Icon
              icon={entry.passed ? Check : X}
              size={12}
              className={cn('shrink-0', entry.passed ? 'text-success' : 'text-danger')}
            />
            <span className="text-text-muted">candidate {entry.candidateIndex + 1}</span>
            <span className="text-text truncate">{entry.taskId}</span>
          </div>
        ))}
        {prunedLog.map((entry, i) => (
          <div key={`pruned-${entry.candidateIndex}-${i}`} className="text-xs text-warning">
            candidate {entry.candidateIndex + 1} pruned — {entry.reason}
          </div>
        ))}
      </div>
    </div>
  );
};

export default TuneLiveProgress;
