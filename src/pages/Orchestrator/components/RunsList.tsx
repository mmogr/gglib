/**
 * RunsList — Sidebar entry showing resumable orchestrator runs.
 *
 * Lists runs from GET /api/council/runs, with status filter tabs.
 * Clicking a run fires onSelectRun so the parent page can resume it.
 */

import { FC, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Button } from '../../../components/ui/Button';
import { Icon } from '../../../components/ui/Icon';
import type { CouncilRun, OrchestratorRunStatus } from '../../../types/orchestrator';

interface RunsListProps {
  runs: CouncilRun[];
  loading: boolean;
  onRefresh: () => void;
  onSelectRun: (run: CouncilRun) => void;
}

const STATUS_FILTERS: Array<{ label: string; value: OrchestratorRunStatus | 'all' }> = [
  { label: 'All', value: 'all' },
  { label: 'Running', value: 'running' },
  { label: 'Paused', value: 'awaiting_approval' },
  { label: 'Done', value: 'completed' },
  { label: 'Failed', value: 'failed' },
];

function statusBadge(status: OrchestratorRunStatus) {
  const base = 'text-xs px-xs py-[2px] rounded-sm font-medium';
  switch (status) {
    case 'running':
      return <span className={cn(base, 'bg-primary/15 text-primary')}>Running</span>;
    case 'awaiting_approval':
      return <span className={cn(base, 'bg-warning/15 text-warning')}>Awaiting approval</span>;
    case 'interrupted':
      return <span className={cn(base, 'bg-warning/15 text-warning')}>Interrupted</span>;
    case 'completed':
      return <span className={cn(base, 'bg-success/15 text-success')}>Completed</span>;
    case 'failed':
      return <span className={cn(base, 'bg-danger/15 text-danger')}>Failed</span>;
    default:
      return <span className={cn(base, 'bg-surface-elevated text-text-muted')}>{status}</span>;
  }
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

const RunsList: FC<RunsListProps> = ({ runs, loading, onRefresh, onSelectRun }) => {
  const [filter, setFilter] = useState<OrchestratorRunStatus | 'all'>('all');

  // Auto-refresh once on mount if runs is empty
  useEffect(() => {
    if (runs.length === 0 && !loading) {
      onRefresh();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filtered = filter === 'all' ? runs : runs.filter((r) => r.status === filter);

  return (
    <div className="flex flex-col gap-sm">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span className="text-sm font-semibold text-text">Previous runs</span>
        <Button variant="ghost" size="sm" onClick={onRefresh} disabled={loading} iconOnly title="Refresh runs">
          <Icon icon={RefreshCw} size={13} className={cn(loading && 'animate-spin')} />
        </Button>
      </div>

      {/* Status filter tabs */}
      <div className="flex gap-xs flex-wrap">
        {STATUS_FILTERS.map((f) => (
          <button
            key={f.value}
            onClick={() => setFilter(f.value)}
            className={cn(
              'text-xs px-sm py-xs rounded-sm border transition-colors bg-transparent cursor-pointer',
              filter === f.value
                ? 'border-primary/40 text-primary bg-primary/10'
                : 'border-border text-text-muted hover:border-border-hover hover:text-text',
            )}
          >
            {f.label}
          </button>
        ))}
      </div>

      {/* Runs list */}
      {loading && filtered.length === 0 && (
        <p className="text-sm text-text-muted text-center py-md">Loading…</p>
      )}

      {!loading && filtered.length === 0 && (
        <p className="text-sm text-text-muted text-center py-md">No runs found.</p>
      )}

      <div className="flex flex-col gap-xs">
        {filtered.map((run) => (
          <button
            key={run.id}
            className="w-full text-left flex flex-col gap-xs p-sm rounded-base border border-border hover:border-border-hover hover:bg-surface-hover transition-colors cursor-pointer bg-transparent"
            onClick={() => onSelectRun(run)}
          >
            <div className="flex items-center justify-between gap-sm">
              <span className="text-xs font-mono text-text-muted truncate flex-1">{run.id.slice(0, 12)}…</span>
              {statusBadge(run.status)}
            </div>
            <p className="text-sm text-text leading-snug line-clamp-2">{run.goal}</p>
            <span className="text-xs text-text-muted">{formatDate(run.updated_at)}</span>
          </button>
        ))}
      </div>
    </div>
  );
};

export default RunsList;
