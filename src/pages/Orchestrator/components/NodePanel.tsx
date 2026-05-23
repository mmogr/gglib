/**
 * NodePanel — Collapsible panel for a single orchestrator task node.
 *
 * Shows: goal, tool allowlist, streaming text, tool calls, final output,
 * status badge, and quick-action steering buttons.
 */

import { FC, useState, useCallback } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Wrench,
  CheckCircle,
  XCircle,
  Loader,
  MessageSquarePlus,
  GitFork,
  Users,
  RefreshCcw,
} from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { NodeState, NodePhase } from '../../../contexts/OrchestratorContext';
import type { TaskGraph, TaskNode } from '../../../types/orchestrator';
import type { GraphDiff } from '../../../types/orchestrator';

// ─── Quick-action definitions ─────────────────────────────────────────────────

const QUICK_ACTIONS = (nodeId: string) => [
  {
    id: 'add-critic',
    label: 'Add critic',
    Icon: MessageSquarePlus,
    instruction: `Add a critic node that critically reviews the output of "${nodeId}"`,
  },
  {
    id: 'split-parallel',
    label: 'Split into 3 parallel',
    Icon: GitFork,
    instruction: `Split the "${nodeId}" node into 3 parallel sub-tasks`,
  },
  {
    id: 'wrap-team',
    label: 'Wrap in team',
    Icon: Users,
    instruction: `Wrap the "${nodeId}" node in a team`,
  },
  {
    id: 'rerun-feedback',
    label: 'Re-run with feedback',
    Icon: RefreshCcw,
    instruction: `Re-run the "${nodeId}" node with more thorough and detailed analysis`,
  },
] as const;

// ─── Types ────────────────────────────────────────────────────────────────────

interface NodePanelProps {
  nodeId: string;
  node: TaskNode;
  nodeState: NodeState | undefined;
  defaultOpen?: boolean;
  /** Current task graph (required for quick-action steer calls). */
  graph?: TaskGraph;
  /** Backend port for the steer / note API. */
  port?: number;
  /** Optional model override for steer calls. */
  model?: string;
  /** Active run id; if present, quick actions fire a note (fire-and-forget). */
  runId?: string | null;
  /** Whether a run is currently active (shows quick actions). */
  isActive?: boolean;
}

type QuickActionState =
  | { kind: 'idle' }
  | { kind: 'pending' }
  | { kind: 'note_sent' }
  | { kind: 'diff_preview'; diff: GraphDiff; instruction: string }
  | { kind: 'error'; message: string };

// ─── Status helpers ───────────────────────────────────────────────────────────

function statusColor(phase: NodePhase): string {
  switch (phase) {
    case 'running':
    case 'compacting':
      return 'border-primary/40 bg-primary/5';
    case 'done':
      return 'border-success/40 bg-success/5';
    case 'failed':
      return 'border-danger/40 bg-danger/5';
    default:
      return 'border-border bg-surface';
  }
}

function statusLabel(phase: NodePhase) {
  const base = 'text-xs font-medium px-xs py-[2px] rounded-sm';
  switch (phase) {
    case 'running':
      return <span className={cn(base, 'bg-primary/15 text-primary')}>Running</span>;
    case 'compacting':
      return <span className={cn(base, 'bg-warning/15 text-warning')}>Compacting</span>;
    case 'done':
      return <span className={cn(base, 'bg-success/15 text-success')}>Done</span>;
    case 'failed':
      return <span className={cn(base, 'bg-danger/15 text-danger')}>Failed</span>;
    default:
      return <span className={cn(base, 'bg-surface-elevated text-text-muted')}>Pending</span>;
  }
}

// ─── NodePanel ────────────────────────────────────────────────────────────────

const NodePanel: FC<NodePanelProps> = ({
  nodeId,
  node,
  nodeState,
  defaultOpen = false,
  graph,
  port = 9000,
  model,
  runId,
  isActive,
}) => {
  const [open, setOpen] = useState(defaultOpen);
  const [qaState, setQaState] = useState<QuickActionState>({ kind: 'idle' });
  const phase: NodePhase = nodeState?.phase ?? 'pending';

  const handleQuickAction = useCallback(
    async (instruction: string) => {
      if (!graph) return;
      setQaState({ kind: 'pending' });
      try {
        if (runId) {
          // Active run: inject steering note (fire-and-forget)
          const res = await fetch(`/api/orchestrator/runs/${runId}/note`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ instruction }),
          });
          if (!res.ok) {
            const text = await res.text();
            setQaState({ kind: 'error', message: text || `HTTP ${res.status}` });
          } else {
            setQaState({ kind: 'note_sent' });
            setTimeout(() => setQaState({ kind: 'idle' }), 2500);
          }
        } else {
          // No active run: propose a diff via steer
          const res = await fetch('/api/orchestrator/steer', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ graph, instruction, port, ...(model ? { model } : {}) }),
          });
          if (!res.ok) {
            const text = await res.text();
            setQaState({ kind: 'error', message: text || `HTTP ${res.status}` });
          } else {
            const data = (await res.json()) as { diff: GraphDiff };
            setQaState({ kind: 'diff_preview', diff: data.diff, instruction });
          }
        }
      } catch (err) {
        setQaState({
          kind: 'error',
          message: err instanceof Error ? err.message : 'Network error',
        });
      }
    },
    [graph, port, model, runId],
  );

  const quickActions = QUICK_ACTIONS(nodeId);

  return (
    <div className={cn('rounded-base border transition-colors', statusColor(phase))}>
      {/* Header */}
      <button
        className="w-full flex items-center gap-sm p-sm text-left bg-transparent border-none cursor-pointer"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={`${open ? 'Collapse' : 'Expand'} node ${nodeId}`}
      >
        <span className="text-text-secondary shrink-0">
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
        <span className="font-mono text-sm font-semibold text-text flex-1 truncate">{nodeId}</span>
        {phase === 'running' || phase === 'compacting' ? (
          <Loader size={12} className="text-primary animate-spin shrink-0" />
        ) : phase === 'done' ? (
          <CheckCircle size={12} className="text-success shrink-0" />
        ) : phase === 'failed' ? (
          <XCircle size={12} className="text-danger shrink-0" />
        ) : null}
        {statusLabel(phase)}
      </button>

      {/* Body */}
      {open && (
        <div className="px-sm pb-sm flex flex-col gap-sm border-t border-border/50">
          {/* Goal */}
          <div className="pt-sm">
            <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Goal</p>
            <p className="text-sm text-text leading-relaxed">{node.goal}</p>
          </div>

          {/* Tool allowlist */}
          {node.tool_allowlist.length > 0 && (
            <div>
              <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Allowed tools</p>
              <div className="flex flex-wrap gap-xs">
                {node.tool_allowlist.map((t) => (
                  <span key={t} className="flex items-center gap-xs text-xs bg-surface-elevated text-text-secondary px-xs py-[2px] rounded-sm font-mono">
                    <Wrench size={10} />
                    {t}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Tool calls log */}
          {nodeState && nodeState.toolLog.length > 0 && (
            <div>
              <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Tool calls</p>
              <div className="flex flex-col gap-xs">
                {nodeState.toolLog.map((tc, i) => (
                  <div key={i} className="flex items-start gap-xs text-xs bg-surface-elevated rounded-sm px-sm py-xs">
                    {tc.done ? (
                      <CheckCircle size={10} className="text-success mt-[2px] shrink-0" />
                    ) : (
                      <Loader size={10} className="text-primary animate-spin mt-[2px] shrink-0" />
                    )}
                    <span className="font-medium text-text font-mono">{tc.displayName}</span>
                    <span className="text-text-muted flex-1 truncate">{tc.argsSummary}</span>
                    {tc.durationDisplay && (
                      <span className="text-text-muted shrink-0">{tc.durationDisplay}</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Streaming text */}
          {nodeState && nodeState.text && (
            <div>
              <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Output</p>
              <pre className="text-sm text-text whitespace-pre-wrap font-mono leading-relaxed bg-surface rounded-sm p-sm max-h-[200px] overflow-y-auto scrollbar-thin">
                {nodeState.text}
              </pre>
            </div>
          )}

          {/* Final output preview (collapsed text) */}
          {nodeState?.outputPreview && nodeState.phase === 'done' && !nodeState.text && (
            <div>
              <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Output preview</p>
              <p className="text-sm text-text-secondary leading-relaxed">{nodeState.outputPreview}</p>
            </div>
          )}

          {/* Error */}
          {nodeState?.error && (
            <div className="rounded-sm bg-danger/10 border border-danger/20 p-sm">
              <p className="text-xs text-danger font-medium">{nodeState.error}</p>
            </div>
          )}

          {/* ── Quick actions ── */}
          {graph && (
            <div>
              <p className="text-xs text-text-muted mb-xs font-medium uppercase tracking-wide">Quick actions</p>
              <div className="flex flex-wrap gap-xs">
                {quickActions.map(({ id, label, Icon, instruction }) => (
                  <button
                    key={id}
                    type="button"
                    aria-label={`${label} for node ${nodeId}`}
                    data-testid={`qa-${id}-${nodeId}`}
                    disabled={qaState.kind === 'pending'}
                    onClick={() => handleQuickAction(instruction)}
                    className={cn(
                      'flex items-center gap-xs text-xs px-sm py-xs rounded-sm border transition-colors',
                      'bg-surface-elevated border-border text-text-secondary',
                      'hover:border-primary/40 hover:text-primary hover:bg-primary/5',
                      'disabled:opacity-50 disabled:cursor-not-allowed',
                      isActive && 'hover:border-warning/40 hover:text-warning hover:bg-warning/5',
                    )}
                  >
                    <Icon size={10} aria-hidden="true" />
                    {label}
                  </button>
                ))}
              </div>

              {/* Quick-action feedback */}
              {qaState.kind === 'pending' && (
                <div className="flex items-center gap-xs mt-xs text-xs text-text-muted">
                  <Loader size={10} className="animate-spin" />
                  <span>Sending…</span>
                </div>
              )}
              {qaState.kind === 'note_sent' && (
                <p className="mt-xs text-xs text-success">
                  ✓ Steering note sent to active run.
                </p>
              )}
              {qaState.kind === 'error' && (
                <p className="mt-xs text-xs text-danger">Error: {qaState.message}</p>
              )}
              {qaState.kind === 'diff_preview' && (
                <div className="mt-sm rounded-sm border border-border bg-surface p-sm flex flex-col gap-xs">
                  <p className="text-xs font-medium text-text-muted uppercase tracking-wide">
                    Proposed changes
                  </p>
                  <p className="text-xs text-text-secondary italic">{qaState.instruction}</p>
                  <pre className="text-xs text-text font-mono whitespace-pre-wrap max-h-[160px] overflow-y-auto scrollbar-thin">
                    {JSON.stringify(qaState.diff, null, 2)}
                  </pre>
                  <div className="flex gap-xs mt-xs">
                    <button
                      type="button"
                      onClick={() => setQaState({ kind: 'idle' })}
                      className="text-xs px-sm py-xs rounded-sm border border-border text-text-secondary hover:bg-surface-hover"
                    >
                      Discard
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default NodePanel;
