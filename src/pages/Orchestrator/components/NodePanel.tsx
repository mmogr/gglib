/**
 * NodePanel — Collapsible panel for a single orchestrator task node.
 *
 * Shows: goal, tool allowlist, streaming text, tool calls, final output,
 * and status badge. Expands/collapses on click.
 */

import { FC, useState } from 'react';
import { ChevronDown, ChevronRight, Wrench, CheckCircle, XCircle, Loader } from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { NodeState, NodePhase } from '../../../contexts/OrchestratorContext';
import type { TaskNode } from '../../../types/orchestrator';

interface NodePanelProps {
  nodeId: string;
  node: TaskNode;
  nodeState: NodeState | undefined;
  defaultOpen?: boolean;
}

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

const NodePanel: FC<NodePanelProps> = ({ nodeId, node, nodeState, defaultOpen = false }) => {
  const [open, setOpen] = useState(defaultOpen);
  const phase: NodePhase = nodeState?.phase ?? 'pending';

  return (
    <div className={cn('rounded-base border transition-colors', statusColor(phase))}>
      {/* Header */}
      <button
        className="w-full flex items-center gap-sm p-sm text-left bg-transparent border-none cursor-pointer"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
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
        </div>
      )}
    </div>
  );
};

export default NodePanel;
