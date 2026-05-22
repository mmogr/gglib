/**
 * DagView — Simple indented-tree visualization of a TaskGraph.
 *
 * Renders each TaskNode with its status badge and dependency structure
 * using pure CSS — no external graph library required.
 */

import { FC } from 'react';
import { CheckCircle, Circle, Loader, AlertCircle, Clock } from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { TaskGraph } from '../../../types/orchestrator';
import type { NodeState, NodePhase } from '../../../contexts/OrchestratorContext';

interface DagViewProps {
  graph: TaskGraph;
  nodeStates: Record<string, NodeState>;
  onSelectNode?: (nodeId: string) => void;
  selectedNodeId?: string | null;
}

function phaseIcon(phase: NodePhase) {
  switch (phase) {
    case 'running':
      return <Loader size={14} className="text-primary animate-spin" />;
    case 'compacting':
      return <Loader size={14} className="text-warning animate-spin" />;
    case 'done':
      return <CheckCircle size={14} className="text-success" />;
    case 'failed':
      return <AlertCircle size={14} className="text-danger" />;
    default:
      return <Circle size={14} className="text-text-muted" />;
  }
}

function phaseBadge(phase: NodePhase) {
  const base = 'text-xs px-xs py-[2px] rounded-sm font-medium';
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

/** Topological sort of node ids so roots come first. */
function topoSort(nodes: TaskGraph['nodes']): string[] {
  const visited = new Set<string>();
  const result: string[] = [];

  function visit(id: string) {
    if (visited.has(id)) return;
    visited.add(id);
    const node = nodes[id];
    if (node) {
      for (const dep of node.depends_on) visit(dep);
    }
    result.push(id);
  }

  for (const id of Object.keys(nodes)) {
    visit(id);
  }
  return result;
}

const DagView: FC<DagViewProps> = ({ graph, nodeStates, onSelectNode, selectedNodeId }) => {
  const sortedIds = topoSort(graph.nodes);

  return (
    <div className="flex flex-col gap-xs">
      {sortedIds.map((id) => {
        const node = graph.nodes[id];
        if (!node) return null;
        const ns = nodeStates[id];
        const phase: NodePhase = ns?.phase ?? 'pending';
        const isSelected = selectedNodeId === id;
        const hasDeps = node.depends_on.length > 0;

        return (
          <div key={id}>
            {hasDeps && (
              <div className="flex items-center gap-xs ml-md mb-xs">
                {node.depends_on.map((dep) => (
                  <span key={dep} className="text-xs text-text-muted flex items-center gap-xs">
                    <Clock size={10} />
                    <span>after <span className="font-medium text-text-secondary">{dep}</span></span>
                  </span>
                ))}
              </div>
            )}
            <button
              className={cn(
                'w-full text-left flex items-start gap-sm p-sm rounded-base border transition-all cursor-pointer bg-transparent',
                isSelected
                  ? 'border-primary/40 bg-primary/5'
                  : 'border-border hover:border-border-hover hover:bg-surface-hover',
              )}
              onClick={() => onSelectNode?.(id)}
            >
              <span className="mt-[2px] shrink-0">{phaseIcon(phase)}</span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-sm flex-wrap">
                  <span className="text-sm font-semibold text-text font-mono">{id}</span>
                  {phaseBadge(phase)}
                </div>
                <p className="text-sm text-text-secondary mt-xs leading-relaxed truncate">{node.goal}</p>
                {node.tool_allowlist.length > 0 && (
                  <div className="flex flex-wrap gap-xs mt-xs">
                    {node.tool_allowlist.map((t) => (
                      <span key={t} className="text-xs bg-surface-elevated text-text-muted px-xs py-[1px] rounded-sm font-mono">
                        {t}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </button>
          </div>
        );
      })}
    </div>
  );
};

export default DagView;
