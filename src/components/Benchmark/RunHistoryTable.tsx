import type { FC } from 'react';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';
import { formatDate } from './format';
import type { BenchmarkRun } from '../../types/benchmark';

interface RunHistoryTableProps {
  history: BenchmarkRun[];
  loading: boolean;
  onRefresh: () => void;
}

/**
 * The "Recent Runs" strip. Lives at page level so every mode shows it —
 * previously it sat inside the perf/compare branch and tune had no history.
 */
export const RunHistoryTable: FC<RunHistoryTableProps> = ({ history, loading, onRefresh }) => (
  <section className="border-t border-border-light shrink-0">
    <div className="flex items-center gap-sm px-base py-sm border-b border-border-light">
      <h2 className="text-sm font-semibold text-text m-0 flex-1">Recent Runs</h2>
      <Button variant="ghost" size="sm" onClick={onRefresh} disabled={loading}>
        {loading ? 'Loading…' : 'Refresh'}
      </Button>
    </div>
    <div className="overflow-x-auto max-h-[220px] overflow-y-auto">
      {history.length === 0 ? (
        <p className="text-xs text-text-muted p-base">No benchmark runs yet.</p>
      ) : (
        <table className="w-full text-xs border-collapse">
          <thead className="sticky top-0 bg-background z-10">
            <tr className="text-left text-text-muted border-b border-border">
              <th className="px-base py-xs font-medium">ID</th>
              <th className="px-base py-xs font-medium">Type</th>
              <th className="px-base py-xs font-medium">Status</th>
              <th className="px-base py-xs font-medium">Models</th>
              <th className="px-base py-xs font-medium">Started</th>
            </tr>
          </thead>
          <tbody>
            {history.map((run) => (
              <tr
                key={run.id}
                className="border-b border-border-light hover:bg-surface-elevated transition-colors"
              >
                <td className="px-base py-xs text-text-secondary font-mono tabular-nums">{run.id}</td>
                <td className="px-base py-xs text-text-secondary">{run.run_type}</td>
                <td className="px-base py-xs">
                  <span className="inline-flex items-center gap-xs">
                    {run.status === 'running' && (
                      <span aria-hidden className="w-1.5 h-1.5 rounded-full bg-success animate-pulse" />
                    )}
                    <span
                      className={cn(
                        'text-text-muted',
                        run.status === 'failed' && 'text-danger',
                        run.status === 'running' && 'text-text',
                      )}
                    >
                      {run.status}
                    </span>
                  </span>
                </td>
                <td className="px-base py-xs text-text-secondary font-mono tabular-nums">
                  {run.model_ids.length}
                </td>
                <td className="px-base py-xs text-text-muted font-mono tabular-nums">
                  {formatDate(run.created_at)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  </section>
);
