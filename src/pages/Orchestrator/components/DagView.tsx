/**
 * DagView — Indented-tree visualization of a TaskGraph with collapsible teams.
 *
 * Renders leaf nodes as clickable rows and Team nodes as collapsible group
 * headers that expand to show their subgraph inline (indented).
 *
 * Expanded state is persisted to `sessionStorage` under a key derived from
 * `runId` (when provided) so the view survives hot-reloads in development.
 *
 * @module pages/Orchestrator/components/DagView
 */

import { FC, useState, useCallback, useEffect } from 'react';
import { CheckCircle, Circle, Loader, AlertCircle, Clock, ChevronDown, ChevronRight, Users } from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { TaskGraph, TaskNodeKind } from '../../../types/orchestrator';
import type { NodeState, NodePhase } from '../../../contexts/CouncilContext';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function isTeamKind(kind: TaskNodeKind | null | undefined): boolean {
  return typeof kind === 'object' && kind !== null && 'team' in kind;
}

function phaseIcon(phase: NodePhase) {
  switch (phase) {
    case 'running':
      return <Loader size={14} className="text-primary animate-spin" aria-label="Running" />;
    case 'compacting':
      return <Loader size={14} className="text-warning animate-spin" aria-label="Compacting" />;
    case 'done':
      return <CheckCircle size={14} className="text-success" aria-label="Done" />;
    case 'failed':
      return <AlertCircle size={14} className="text-danger" aria-label="Failed" />;
    default:
      return <Circle size={14} className="text-text-muted" aria-label="Pending" />;
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

// ─── sessionStorage helpers ───────────────────────────────────────────────────

function storageKey(runId?: string | null): string {
  return `orch_dag_expanded_${runId ?? 'default'}`;
}

function loadExpandedIds(runId?: string | null): Set<string> {
  try {
    const raw = sessionStorage.getItem(storageKey(runId));
    if (raw) return new Set(JSON.parse(raw) as string[]);
  } catch {
    // ignore
  }
  return new Set();
}

function saveExpandedIds(runId: string | null | undefined, ids: Set<string>): void {
  try {
    sessionStorage.setItem(storageKey(runId), JSON.stringify([...ids]));
  } catch {
    // ignore
  }
}

// ─── DagView ─────────────────────────────────────────────────────────────────

export interface DagViewProps {
  graph: TaskGraph;
  nodeStates: Record<string, NodeState>;
  onSelectNode?: (nodeId: string) => void;
  selectedNodeId?: string | null;
  /** Optional run id used to scope sessionStorage persistence. */
  runId?: string | null;
  /** Indentation depth (used for recursive team rendering). */
  depth?: number;
}

const DagView: FC<DagViewProps> = ({
  graph,
  nodeStates,
  onSelectNode,
  selectedNodeId,
  runId,
  depth = 0,
}) => {
  const [expandedTeams, setExpandedTeams] = useState<Set<string>>(() =>
    loadExpandedIds(runId),
  );

  // Re-load when runId changes (resume of a different run).
  useEffect(() => {
    setExpandedTeams(loadExpandedIds(runId));
  }, [runId]);

  const toggleTeam = useCallback(
    (id: string) => {
      setExpandedTeams((prev) => {
        const next = new Set(prev);
        if (next.has(id)) {
          next.delete(id);
        } else {
          next.add(id);
        }
        saveExpandedIds(runId, next);
        return next;
      });
    },
    [runId],
  );

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
        const isTeam = isTeamKind(node.kind);
        const isExpanded = expandedTeams.has(id);

        return (
          <div key={id} style={{ paddingLeft: depth > 0 ? `${depth * 16}px` : undefined }}>
            {hasDeps && (
              <div className="flex items-center gap-xs ml-md mb-xs">
                {node.depends_on.map((dep) => (
                  <span key={dep} className="text-xs text-text-muted flex items-center gap-xs">
                    <Clock size={10} aria-hidden="true" />
                    <span>
                      after{' '}
                      <span className="font-medium text-text-secondary">{dep}</span>
                    </span>
                  </span>
                ))}
              </div>
            )}

            {isTeam ? (
              /* ── Team node: collapsible group header ── */
              <div>
                <button
                  type="button"
                  className={cn(
                    'w-full text-left flex items-start gap-sm p-sm rounded-base border transition-all cursor-pointer bg-transparent',
                    'border-border hover:border-primary/40 hover:bg-primary/5',
                  )}
                  onClick={() => toggleTeam(id)}
                  aria-expanded={isExpanded}
                  aria-label={`${isExpanded ? 'Collapse' : 'Expand'} team ${id}`}
                  data-testid={`team-header-${id}`}
                >
                  <span className="mt-[2px] shrink-0">
                    {isExpanded ? (
                      <ChevronDown size={14} className="text-text-muted" />
                    ) : (
                      <ChevronRight size={14} className="text-text-muted" />
                    )}
                  </span>
                  <span className="mt-[2px] shrink-0">
                    <Users size={14} className="text-primary" aria-hidden="true" />
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-sm flex-wrap">
                      <span className="text-sm font-semibold text-text font-mono">{id}</span>
                      <span className="text-xs text-text-muted">Team</span>
                      {phaseBadge(phase)}
                    </div>
                    <p className="text-sm text-text-secondary mt-xs leading-relaxed truncate">
                      {node.goal}
                    </p>
                  </div>
                  <span className="mt-[2px] shrink-0">{phaseIcon(phase)}</span>
                </button>

                {/* Expanded subgraph */}
                {isExpanded && typeof node.kind === 'object' && node.kind !== null && 'team' in node.kind && (
                  <div
                    className="mt-xs ml-sm border-l-2 border-border pl-sm"
                    data-testid={`team-subgraph-${id}`}
                  >
                    <DagView
                      graph={node.kind.team.subgraph}
                      nodeStates={nodeStates}
                      selectedNodeId={selectedNodeId}
                      onSelectNode={onSelectNode}
                      runId={runId}
                      depth={depth + 1}
                    />
                  </div>
                )}
              </div>
            ) : (
              /* ── Leaf node ── */
              <button
                type="button"
                className={cn(
                  'w-full text-left flex items-start gap-sm p-sm rounded-base border transition-all cursor-pointer bg-transparent',
                  isSelected
                    ? 'border-primary/40 bg-primary/5'
                    : 'border-border hover:border-border-hover hover:bg-surface-hover',
                )}
                onClick={() => onSelectNode?.(id)}
                aria-pressed={isSelected}
                aria-label={`Select node ${id}`}
                data-testid={`dag-node-${id}`}
              >
                <span className="mt-[2px] shrink-0">{phaseIcon(phase)}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-sm flex-wrap">
                    <span className="text-sm font-semibold text-text font-mono">{id}</span>
                    {phaseBadge(phase)}
                  </div>
                  <p className="text-sm text-text-secondary mt-xs leading-relaxed truncate">
                    {node.goal}
                  </p>
                  {node.tool_allowlist.length > 0 && (
                    <div className="flex flex-wrap gap-xs mt-xs">
                      {node.tool_allowlist.map((t) => (
                        <span
                          key={t}
                          className="text-xs bg-surface-elevated text-text-muted px-xs py-[1px] rounded-sm font-mono"
                        >
                          {t}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
};

export default DagView;
