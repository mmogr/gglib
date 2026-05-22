/**
 * OrchestratorPlanPreview — minimal Tauri / web page for the Director planner.
 *
 * Allows the user to type a goal and port, then streams the resulting task
 * graph via `POST /api/orchestrator/plan` (HTTP SSE — no tauri::command).
 *
 * Renders:
 *   - Live replan-attempt feedback while the director is thinking.
 *   - The final task graph as an indented node tree with dependency labels.
 *   - An error banner when planning fails.
 */

import { useState, useRef, useCallback } from 'react';
import { planOrchestrator } from '../services/clients/orchestrator';
import type {
  OrchestratorEvent,
  PlanProposedEvent,
  ReplanAttemptEvent,
  TaskGraph,
  TaskNode,
} from '../types/orchestrator';

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  onBack?: () => void;
}

// ─── Component ───────────────────────────────────────────────────────────────

export default function OrchestratorPlanPreview({ onBack }: Props) {
  const [goal, setGoal] = useState('');
  const [port, setPort] = useState('9000');
  const [model, setModel] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [replans, setReplans] = useState<ReplanAttemptEvent[]>([]);
  const [graph, setGraph] = useState<TaskGraph | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!goal.trim()) return;

      // Cancel any in-flight request.
      abortRef.current?.abort();
      const ctrl = new AbortController();
      abortRef.current = ctrl;

      setLoading(true);
      setError(null);
      setReplans([]);
      setGraph(null);

      try {
        await planOrchestrator(
          {
            goal: goal.trim(),
            port: parseInt(port, 10) || 9000,
            model: model.trim() || undefined,
          },
          (event: OrchestratorEvent) => {
            if (event.type === 'replan_attempt') {
              setReplans((prev) => [...prev, event as ReplanAttemptEvent]);
            } else if (event.type === 'plan_proposed') {
              setGraph((event as PlanProposedEvent).graph);
            } else if (event.type === 'orchestrator_error') {
              setError(event.message);
            }
          },
          ctrl.signal,
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          setError(err.message);
        }
      } finally {
        setLoading(false);
      }
    },
    [goal, port, model],
  );

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    setLoading(false);
  }, []);

  return (
    <div className="p-lg max-w-[860px] mx-auto">
      {/* Header */}
      <div className="flex items-center gap-base mb-5">
        {onBack && (
          <button
            onClick={onBack}
            className="bg-transparent border border-border-hover rounded px-[10px] py-xs cursor-pointer text-text text-sm hover:border-border-focus"
          >
            ← Back
          </button>
        )}
        <h2 className="m-0 text-xl font-semibold">Orchestrator — Plan Preview</h2>
      </div>

      {/* Form */}
      <form onSubmit={handleSubmit} className="flex flex-col gap-md mb-5">
        <div className="flex flex-col">
          <label className="flex flex-col gap-xs text-sm text-text-secondary">
            Goal
            <textarea
              value={goal}
              onChange={(e) => setGoal(e.target.value)}
              placeholder="Describe the high-level goal to decompose…"
              rows={3}
              className="resize-y p-sm rounded border border-border bg-background-input text-text text-sm min-w-[400px] disabled:opacity-60"
              disabled={loading}
            />
          </label>
        </div>
        <div className="flex gap-base flex-wrap">
          <label className="flex flex-col gap-xs text-sm text-text-secondary">
            Server port
            <input
              type="number"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              min={1}
              max={65535}
              className="px-sm py-[6px] rounded border border-border bg-background-input text-text text-sm w-[90px] disabled:opacity-60"
              disabled={loading}
            />
          </label>
          <label className="flex flex-col gap-xs text-sm text-text-secondary">
            Model (optional)
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="default"
              className="px-sm py-[6px] rounded border border-border bg-background-input text-text text-sm w-[200px] disabled:opacity-60"
              disabled={loading}
            />
          </label>
        </div>
        <div className="flex gap-sm items-center">
          <button
            type="submit"
            disabled={loading || !goal.trim()}
            className="px-5 py-sm rounded border-none bg-primary text-white font-semibold cursor-pointer text-sm hover:bg-primary-hover disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {loading ? 'Planning…' : 'Plan'}
          </button>
          {loading && (
            <button
              type="button"
              onClick={handleCancel}
              className="px-base py-sm rounded border border-border-hover bg-transparent text-text-secondary cursor-pointer text-sm hover:border-border-focus"
            >
              Cancel
            </button>
          )}
        </div>
      </form>

      {/* Replan feedback */}
      {replans.length > 0 && (
        <div className="bg-warning-subtle border border-warning-border rounded px-base py-md mb-base text-sm text-warning">
          <strong>Replan attempts</strong>
          {replans.map((r, i) => (
            <div key={i} className="mt-xs pl-md">
              #{r.attempt}: {r.reason}
            </div>
          ))}
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="bg-danger-subtle border border-danger-border rounded px-base py-md mb-base text-danger text-sm">
          {error}
        </div>
      )}

      {/* Graph */}
      {graph && <GraphView graph={graph} />}
    </div>
  );
}

// ─── Graph renderer ──────────────────────────────────────────────────────────

function GraphView({ graph }: { graph: TaskGraph }) {
  const ordered = topoSort(graph.nodes);

  return (
    <div className="bg-surface border border-border rounded-md px-5 py-base">
      <div className="mb-md text-base text-text">
        <strong>Goal:</strong> {graph.goal}
      </div>
      <div className="flex flex-col gap-xs font-mono">
        {ordered.map((id, i) => {
          const node = graph.nodes[id];
          const isLast = i === ordered.length - 1;
          return <NodeRow key={id} id={id} node={node} isLast={isLast} />;
        })}
      </div>
    </div>
  );
}

function NodeRow({
  id,
  node,
  isLast,
}: {
  id: string;
  node: TaskNode;
  isLast: boolean;
}) {
  const connector = isLast ? '└──' : '├──';
  const deps =
    node.depends_on.length > 0 ? ` (needs: ${node.depends_on.join(', ')})` : '';

  return (
    <div className="flex gap-[6px] items-baseline flex-wrap">
      <span className="text-text-disabled select-none min-w-[2.5rem]">{connector}</span>
      <span className="text-primary-light font-semibold">[{id}]</span>
      <span className="text-text">{node.goal}</span>
      {deps && <span className="text-text-muted text-xs">{deps}</span>}
    </div>
  );
}

// ─── Topological sort ────────────────────────────────────────────────────────

function topoSort(nodes: Record<string, TaskNode>): string[] {
  const inDegree: Record<string, number> = {};
  const dependents: Record<string, string[]> = {};

  for (const id of Object.keys(nodes)) {
    inDegree[id] = 0;
  }
  for (const [id, node] of Object.entries(nodes)) {
    for (const dep of node.depends_on) {
      inDegree[id] = (inDegree[id] ?? 0) + 1;
      (dependents[dep] ??= []).push(id);
    }
  }

  const queue = Object.keys(nodes)
    .filter((id) => inDegree[id] === 0)
    .sort();

  const result: string[] = [];
  const visited = new Set<string>();

  while (queue.length > 0) {
    const id = queue.shift()!;
    if (visited.has(id)) continue;
    visited.add(id);
    result.push(id);

    const children = (dependents[id] ?? []).slice().sort();
    for (const child of children) {
      inDegree[child]--;
      if (inDegree[child] === 0 && !visited.has(child)) {
        queue.push(child);
        queue.sort();
      }
    }
  }

  return result;
}
