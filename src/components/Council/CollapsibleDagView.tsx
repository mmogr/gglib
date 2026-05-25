/**
 * CollapsibleDagView — collapsed-by-default wrapper for the DagView.
 *
 * Collapsed summary: chevron · "DAG" label · phase-count pills · mini progress bar
 * Expanded: full DagView indented tree (the unchanged existing component)
 *
 * The progress bar and counts are derived from `nodeStates` so they update
 * reactively as the run streams. The full DagView is not mounted until the
 * user expands, keeping layout cost low for long chat histories.
 *
 * Internal expand/collapse state managed here. Pass `defaultExpanded` to
 * override. The expanded-team set inside the nested DagView is preserved by
 * DagView's own sessionStorage persistence, keyed by `runId`.
 *
 * @module components/Council/CollapsibleDagView
 */

import { type FC, useId, useState } from 'react';
import {
  AlertCircle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Loader,
  Network,
} from 'lucide-react';
import { cn } from '../../utils/cn';
import type { NodeState } from '../../contexts/CouncilContext';
import type { TaskGraph } from '../../types/council';
import DagView from '../../pages/Council/components/DagView';

// ─── Node count derivation ────────────────────────────────────────────────────

interface NodeCounts {
  total: number;
  done: number;
  running: number;
  failed: number;
  pending: number;
}

function deriveNodeCounts(
  graph: TaskGraph,
  nodeStates: Record<string, NodeState>,
): NodeCounts {
  const ids = Object.keys(graph.nodes);
  let done = 0;
  let running = 0;
  let failed = 0;

  for (const id of ids) {
    const p = nodeStates[id]?.phase;
    if (p === 'done') done++;
    else if (p === 'running' || p === 'compacting') running++;
    else if (p === 'failed') failed++;
  }

  return {
    total: ids.length,
    done,
    running,
    failed,
    pending: ids.length - done - running - failed,
  };
}

// ─── Summary bar ──────────────────────────────────────────────────────────────

interface SummaryBarProps {
  counts: NodeCounts;
}

/**
 * A thin (4 px) segmented progress bar. Each segment fills proportionally:
 * done=green, running=blue (animated), failed=red, pending=surface.
 */
const SummaryBar: FC<SummaryBarProps> = ({ counts }) => {
  const { total, done, running, failed, pending } = counts;
  if (total === 0) return null;

  const pct = (n: number) => `${((n / total) * 100).toFixed(1)}%`;

  return (
    <div
      className="flex h-1 w-24 rounded-full overflow-hidden bg-surface-elevated shrink-0"
      role="progressbar"
      aria-valuenow={done}
      aria-valuemin={0}
      aria-valuemax={total}
      aria-label={`${done} of ${total} nodes done`}
      data-testid="dag-summary-bar"
    >
      {done > 0 && (
        <span
          className="h-full bg-success transition-[width] duration-300"
          style={{ width: pct(done) }}
        />
      )}
      {running > 0 && (
        <span
          className="h-full bg-primary animate-pulse"
          style={{ width: pct(running) }}
        />
      )}
      {failed > 0 && (
        <span
          className="h-full bg-danger"
          style={{ width: pct(failed) }}
        />
      )}
      {pending > 0 && (
        <span
          className="h-full bg-border"
          style={{ width: pct(pending) }}
        />
      )}
    </div>
  );
};

// ─── Count pills ──────────────────────────────────────────────────────────────

interface CountPillsProps {
  counts: NodeCounts;
}

const CountPills: FC<CountPillsProps> = ({ counts }) => {
  const { done, running, failed, pending } = counts;

  return (
    <div className="flex items-center gap-xs shrink-0" aria-hidden="true">
      {done > 0 && (
        <span className="flex items-center gap-[3px] text-xs text-success tabular-nums">
          <CheckCircle size={11} aria-hidden="true" />
          {done}
        </span>
      )}
      {running > 0 && (
        <span className="flex items-center gap-[3px] text-xs text-primary tabular-nums">
          <Loader size={11} className="animate-spin" aria-hidden="true" />
          {running}
        </span>
      )}
      {failed > 0 && (
        <span className="flex items-center gap-[3px] text-xs text-danger tabular-nums">
          <AlertCircle size={11} aria-hidden="true" />
          {failed}
        </span>
      )}
      {pending > 0 && done === 0 && running === 0 && failed === 0 && (
        // Only show pending count when nothing else is active — avoids clutter
        <span className="text-xs text-text-muted tabular-nums">{pending} pending</span>
      )}
    </div>
  );
};

// ─── Props ────────────────────────────────────────────────────────────────────

export interface CollapsibleDagViewProps {
  graph: TaskGraph;
  nodeStates: Record<string, NodeState>;
  onSelectNode?: (nodeId: string) => void;
  selectedNodeId?: string | null;
  /** Passed to DagView and its sessionStorage key. */
  runId?: string | null;
  /** When true, the DAG tree opens expanded on first render. Defaults to false. */
  defaultExpanded?: boolean;
}

// ─── Component ────────────────────────────────────────────────────────────────

/**
 * Renders a single-line progress summary header that expands on click to
 * reveal the full DagView indented tree.
 *
 * The collapsed header is O(N) in nodeStates but renders only two small child
 * components (CountPills + SummaryBar). The DagView, which recursively renders
 * the full task graph tree, is only mounted after the user expands.
 */
const CollapsibleDagView: FC<CollapsibleDagViewProps> = ({
  graph,
  nodeStates,
  onSelectNode,
  selectedNodeId,
  runId,
  defaultExpanded = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const headerId = useId();
  const panelId = `${headerId}-panel`;

  const counts = deriveNodeCounts(graph, nodeStates);

  return (
    <div
      className="border border-border rounded-base overflow-hidden"
      data-testid="collapsible-dag-view"
    >
      {/* ── Collapsed header / toggle ──────────────────────────────────────── */}
      <button
        type="button"
        id={headerId}
        onClick={() => setIsExpanded((v) => !v)}
        aria-expanded={isExpanded}
        aria-controls={panelId}
        className={cn(
          'w-full flex items-center gap-sm px-sm py-xs text-left',
          'bg-surface hover:bg-surface-hover transition-colors',
          isExpanded && 'border-b border-border',
        )}
        data-testid="collapsible-dag-view-toggle"
      >
        {/* Chevron */}
        {isExpanded ? (
          <ChevronDown size={14} className="text-text-muted shrink-0" aria-hidden="true" />
        ) : (
          <ChevronRight size={14} className="text-text-muted shrink-0" aria-hidden="true" />
        )}

        {/* DAG icon */}
        <Network size={13} className="text-primary shrink-0" aria-hidden="true" />

        {/* Label */}
        <span className="text-xs font-medium text-text-secondary tabular-nums">
          DAG&nbsp;&middot;&nbsp;{counts.total}&nbsp;{counts.total === 1 ? 'node' : 'nodes'}
        </span>

        {/* Count pills — done/running/failed at a glance */}
        <CountPills counts={counts} />

        {/* Spacer */}
        <span className="flex-1" />

        {/* Progress bar — right-anchored */}
        <SummaryBar counts={counts} />
      </button>

      {/* ── Expanded panel — full DagView tree ────────────────────────────── */}
      {isExpanded && (
        <div
          id={panelId}
          role="region"
          aria-labelledby={headerId}
          className="p-sm"
        >
          <DagView
            graph={graph}
            nodeStates={nodeStates}
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            runId={runId}
          />
        </div>
      )}
    </div>
  );
};

export default CollapsibleDagView;
