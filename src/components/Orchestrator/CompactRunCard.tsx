/**
 * CompactRunCard — single-line run status chip for use inside a chat thread.
 *
 * Designed to fit comfortably within chat message width constraints while still
 * communicating run phase, goal, and aggregate node progress at a glance.
 *
 * The component is a controlled toggle: the parent decides whether the detail
 * view is expanded (and can auto-expand on activity, auto-collapse on
 * completion, etc.).
 *
 * @module components/Orchestrator/CompactRunCard
 */

import { type FC } from 'react';
import {
  AlertCircle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Clock,
  Loader,
} from 'lucide-react';
import { cn } from '../../utils/cn';
import type { NodeState, OrchestratorPhase } from '../../contexts/OrchestratorContext';
import type { TaskGraph } from '../../types/orchestrator';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function deriveNodeCounts(graph: TaskGraph | null, nodeStates: Record<string, NodeState>) {
  if (!graph) return { total: 0, done: 0, running: 0, failed: 0 };
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
  return { total: ids.length, done, running, failed };
}

function phaseStatusIcon(phase: OrchestratorPhase, className?: string) {
  const base = cn('shrink-0', className);
  switch (phase) {
    case 'planning':
    case 'running':
    case 'synthesizing':
      return (
        <Loader
          size={13}
          className={cn(base, 'text-primary animate-spin')}
          aria-label={phase === 'synthesizing' ? 'Synthesizing' : phase === 'planning' ? 'Planning' : 'Running'}
        />
      );
    case 'awaiting_approval':
      return <Clock size={13} className={cn(base, 'text-warning')} aria-label="Awaiting approval" />;
    case 'complete':
      return <CheckCircle size={13} className={cn(base, 'text-success')} aria-label="Complete" />;
    case 'error':
      return <AlertCircle size={13} className={cn(base, 'text-danger')} aria-label="Error" />;
    default:
      return <Clock size={13} className={cn(base, 'text-text-muted')} aria-label="Idle" />;
  }
}

function phaseLabel(
  phase: OrchestratorPhase,
  total: number,
  done: number,
): string {
  switch (phase) {
    case 'planning':
      return 'Planning\u2026';
    case 'running':
      return total > 0 ? `Running \u00b7 ${done}\u202f/\u202f${total} nodes` : 'Running\u2026';
    case 'synthesizing':
      return 'Synthesizing\u2026';
    case 'awaiting_approval':
      return 'Awaiting approval';
    case 'complete':
      return total > 0 ? `Done \u00b7 ${total} nodes` : 'Done';
    case 'error':
      return 'Failed';
    default:
      return 'Idle';
  }
}

function containerClasses(phase: OrchestratorPhase): string {
  switch (phase) {
    case 'planning':
    case 'running':
    case 'synthesizing':
      return 'border-primary/30 bg-primary/5';
    case 'awaiting_approval':
      return 'border-warning/30 bg-warning/5';
    case 'complete':
      return 'border-success/30 bg-success/5';
    case 'error':
      return 'border-danger/30 bg-danger/5';
    default:
      return 'border-border bg-surface';
  }
}

// ─── Props ────────────────────────────────────────────────────────────────────

export interface CompactRunCardProps {
  /** The user-supplied goal text for this run. */
  goal: string;
  /** Current phase of the run. */
  phase: OrchestratorPhase;
  /** Planned graph, or null if planning has not completed yet. */
  graph: TaskGraph | null;
  /** Live node states keyed by node id. */
  nodeStates: Record<string, NodeState>;
  /** Whether the detail view below this chip is currently visible. */
  isExpanded: boolean;
  /** Called when the user clicks the expand / collapse affordance. */
  onToggle: () => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

/**
 * A single-line status chip that acts as the visible header for a collapsed
 * orchestrator run thread.
 *
 * Shows: phase icon · phase label · goal snippet · expand toggle.
 * When `isExpanded`, the parent is expected to render the detail content
 * (CollapsibleCastingSheet, CollapsibleDagView, etc.) below this chip.
 */
const CompactRunCard: FC<CompactRunCardProps> = ({
  goal,
  phase,
  graph,
  nodeStates,
  isExpanded,
  onToggle,
}) => {
  const { total, done } = deriveNodeCounts(graph, nodeStates);

  return (
    <div
      className={cn(
        'flex items-center gap-sm px-sm py-xs rounded-base border text-sm transition-colors',
        containerClasses(phase),
      )}
      data-testid="compact-run-card"
      data-phase={phase}
    >
      {phaseStatusIcon(phase)}

      {/* Phase label — fixed-width feel via shrink-0 */}
      <span className="text-xs font-medium text-text-secondary shrink-0 tabular-nums">
        {phaseLabel(phase, total, done)}
      </span>

      {/* Goal snippet — fills the remaining space, truncates gracefully */}
      <span className="text-xs text-text-muted truncate flex-1 min-w-0" title={goal}>
        {goal}
      </span>

      {/* Expand / collapse toggle */}
      <button
        type="button"
        onClick={onToggle}
        className={cn(
          'flex items-center gap-[3px] shrink-0 text-xs text-text-muted transition-colors',
          'hover:text-text rounded-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary',
        )}
        aria-expanded={isExpanded}
        aria-label={isExpanded ? 'Collapse run details' : 'Expand run details'}
        data-testid="compact-run-card-toggle"
      >
        {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span>{isExpanded ? 'Hide' : 'Show'}</span>
      </button>
    </div>
  );
};

export default CompactRunCard;
