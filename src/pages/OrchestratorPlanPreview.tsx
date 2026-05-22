/**
 * OrchestratorPlanPreview — Director planner + Worker executor page.
 *
 * Phase C: Extends Phase B (plan preview) with:
 *   - An "Execute" button that appears after a plan is accepted.
 *   - Per-node status panels showing streaming text, tool calls, and status.
 *   - A synthesis output area at the bottom.
 *
 * HTTP transport only — no tauri::command.
 */

import { useState, useRef, useCallback } from 'react';
import { planOrchestrator, runOrchestrator } from '../services/clients/orchestrator';
import type {
  NodeCompleteEvent,
  NodeCompactingEvent,
  NodeFailedEvent,
  NodeStartedEvent,
  NodeTextDeltaEvent,
  NodeToolCallCompleteEvent,
  NodeToolCallStartEvent,
  OrchestratorEvent,
  PlanProposedEvent,
  ReplanAttemptEvent,
  TaskGraph,
  TaskNode,
} from '../types/orchestrator';

// ─── Local state types ───────────────────────────────────────────────────────

type NodePhase = 'pending' | 'running' | 'compacting' | 'done' | 'failed';

interface NodeState {
  phase: NodePhase;
  goal: string;
  text: string;
  toolLog: ToolEntry[];
  error?: string;
  outputPreview?: string;
}

interface ToolEntry {
  name: string;
  argsSummary: string;
  durationDisplay?: string;
  done: boolean;
}

// ─── Props ───────────────────────────────────────────────────────────────────

interface Props {
  onBack?: () => void;
}

// ─── Component ───────────────────────────────────────────────────────────────

export default function OrchestratorPlanPreview({ onBack }: Props) {
  const [goal, setGoal] = useState('');
  const [port, setPort] = useState('9000');
  const [model, setModel] = useState('');

  // Plan phase
  const [planning, setPlanning] = useState(false);
  const [planError, setPlanError] = useState<string | null>(null);
  const [replans, setReplans] = useState<ReplanAttemptEvent[]>([]);
  const [graph, setGraph] = useState<TaskGraph | null>(null);

  // Execute phase
  const [executing, setExecuting] = useState(false);
  const [execError, setExecError] = useState<string | null>(null);
  const [nodeStates, setNodeStates] = useState<Record<string, NodeState>>({});
  const [synthesisText, setSynthesisText] = useState('');
  const [synthesisComplete, setSynthesisComplete] = useState(false);
  const [finalAnswer, setFinalAnswer] = useState<string | null>(null);

  const abortRef = useRef<AbortController | null>(null);

  // ── Plan ───────────────────────────────────────────────────────────────────

  const handlePlan = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!goal.trim()) return;

      abortRef.current?.abort();
      const ctrl = new AbortController();
      abortRef.current = ctrl;

      setPlanning(true);
      setPlanError(null);
      setReplans([]);
      setGraph(null);
      setNodeStates({});
      setSynthesisText('');
      setSynthesisComplete(false);
      setFinalAnswer(null);
      setExecError(null);

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
              setPlanError(event.message);
            }
          },
          ctrl.signal,
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          setPlanError(err.message);
        }
      } finally {
        setPlanning(false);
      }
    },
    [goal, port, model],
  );

  // ── Execute ────────────────────────────────────────────────────────────────

  const handleExecute = useCallback(async () => {
    if (!graph) return;

    abortRef.current?.abort();
    const ctrl = new AbortController();
    abortRef.current = ctrl;

    // Reset execution state.
    setExecuting(true);
    setExecError(null);
    setNodeStates({});
    setSynthesisText('');
    setSynthesisComplete(false);
    setFinalAnswer(null);

    try {
      await runOrchestrator(
        {
          goal: goal.trim(),
          port: parseInt(port, 10) || 9000,
          model: model.trim() || undefined,
        },
        (event: OrchestratorEvent) => {
          switch (event.type) {
            case 'node_started': {
              const e = event as NodeStartedEvent;
              setNodeStates((prev) => ({
                ...prev,
                [e.node_id]: {
                  phase: 'running',
                  goal: e.goal,
                  text: '',
                  toolLog: [],
                },
              }));
              break;
            }
            case 'node_text_delta': {
              const e = event as NodeTextDeltaEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                if (!ns) return prev;
                return { ...prev, [e.node_id]: { ...ns, text: ns.text + e.delta } };
              });
              break;
            }
            case 'node_tool_call_start': {
              const e = event as NodeToolCallStartEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                if (!ns) return prev;
                return {
                  ...prev,
                  [e.node_id]: {
                    ...ns,
                    toolLog: [
                      ...ns.toolLog,
                      { name: e.display_name, argsSummary: e.args_summary, done: false },
                    ],
                  },
                };
              });
              break;
            }
            case 'node_tool_call_complete': {
              const e = event as NodeToolCallCompleteEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                if (!ns) return prev;
                const updatedLog = ns.toolLog.map((t) =>
                  !t.done && t.name === e.display_name
                    ? { ...t, done: true, durationDisplay: e.duration_display }
                    : t,
                );
                return { ...prev, [e.node_id]: { ...ns, toolLog: updatedLog } };
              });
              break;
            }
            case 'node_compacting': {
              const e = event as NodeCompactingEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                if (!ns) return prev;
                return { ...prev, [e.node_id]: { ...ns, phase: 'compacting' } };
              });
              break;
            }
            case 'node_complete': {
              const e = event as NodeCompleteEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                if (!ns) return prev;
                return {
                  ...prev,
                  [e.node_id]: { ...ns, phase: 'done', outputPreview: e.output_preview },
                };
              });
              break;
            }
            case 'node_failed': {
              const e = event as NodeFailedEvent;
              setNodeStates((prev) => {
                const ns = prev[e.node_id];
                return {
                  ...prev,
                  [e.node_id]: ns
                    ? { ...ns, phase: 'failed', error: e.error }
                    : { phase: 'failed', goal: e.node_id, text: '', toolLog: [], error: e.error },
                };
              });
              break;
            }
            case 'synthesis_text_delta':
              setSynthesisText((prev) => prev + event.delta);
              break;
            case 'synthesis_complete':
              setSynthesisComplete(true);
              break;
            case 'orchestrator_complete':
              setFinalAnswer(event.answer);
              break;
            case 'orchestrator_error':
              setExecError(event.message);
              break;
            default:
              break;
          }
        },
        ctrl.signal,
      );
    } catch (err: unknown) {
      if (err instanceof Error && err.name !== 'AbortError') {
        setExecError(err.message);
      }
    } finally {
      setExecuting(false);
    }
  }, [goal, port, model, graph]);

  const handleCancel = useCallback(() => {
    abortRef.current?.abort();
    setPlanning(false);
    setExecuting(false);
  }, []);

  const isRunning = planning || executing;

  return (
    <div className="p-lg max-w-[900px] mx-auto">
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
        <h2 className="m-0 text-xl font-semibold">Orchestrator</h2>
      </div>

      {/* Form */}
      <form onSubmit={handlePlan} className="flex flex-col gap-md mb-5">
        <div className="flex flex-col">
          <label className="flex flex-col gap-xs text-sm text-text-secondary">
            Goal
            <textarea
              value={goal}
              onChange={(e) => setGoal(e.target.value)}
              placeholder="Describe the high-level goal to decompose…"
              rows={3}
              className="resize-y p-sm rounded border border-border bg-background-input text-text text-sm min-w-[400px] disabled:opacity-60"
              disabled={isRunning}
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
              disabled={isRunning}
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
              disabled={isRunning}
            />
          </label>
        </div>
        <div className="flex gap-sm items-center">
          <button
            type="submit"
            disabled={isRunning || !goal.trim()}
            className="px-5 py-sm rounded border-none bg-primary text-white font-semibold cursor-pointer text-sm hover:bg-primary-hover disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {planning ? 'Planning…' : 'Plan'}
          </button>
          {graph && !executing && !finalAnswer && (
            <button
              type="button"
              onClick={handleExecute}
              disabled={isRunning}
              className="px-5 py-sm rounded border-none bg-success text-white font-semibold cursor-pointer text-sm hover:opacity-90 disabled:opacity-60 disabled:cursor-not-allowed"
            >
              Execute
            </button>
          )}
          {isRunning && (
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

      {/* Plan errors */}
      {planError && (
        <div className="bg-danger-subtle border border-danger-border rounded px-base py-md mb-base text-danger text-sm">
          {planError}
        </div>
      )}

      {/* Graph */}
      {graph && <GraphView graph={graph} />}

      {/* Node panels */}
      {Object.keys(nodeStates).length > 0 && (
        <div className="mt-5 flex flex-col gap-base">
          <h3 className="m-0 text-base font-semibold">Execution</h3>
          {Object.entries(nodeStates).map(([id, ns]) => (
            <NodePanel key={id} nodeId={id} state={ns} />
          ))}
        </div>
      )}

      {/* Synthesis */}
      {(synthesisText || synthesisComplete) && (
        <div className="mt-5 bg-surface border border-border rounded-md px-5 py-base">
          <div className="text-sm font-semibold text-text-secondary mb-sm">Synthesis</div>
          <div className="text-text text-sm whitespace-pre-wrap">{synthesisText}</div>
        </div>
      )}

      {/* Execution error */}
      {execError && (
        <div className="mt-5 bg-danger-subtle border border-danger-border rounded px-base py-md text-danger text-sm">
          {execError}
        </div>
      )}

      {/* Final answer */}
      {finalAnswer && (
        <div className="mt-5 bg-surface border border-border rounded-md px-5 py-base">
          <div className="text-sm font-semibold text-text-secondary mb-sm">Final Answer</div>
          <div className="text-text text-sm whitespace-pre-wrap">{finalAnswer}</div>
        </div>
      )}
    </div>
  );
}

// ─── NodePanel ────────────────────────────────────────────────────────────────

const NODE_PHASE_BADGE: Record<NodePhase, string> = {
  pending: 'bg-surface text-text-secondary',
  running: 'bg-primary-subtle text-primary',
  compacting: 'bg-warning-subtle text-warning',
  done: 'bg-success-subtle text-success',
  failed: 'bg-danger-subtle text-danger',
};

const NODE_PHASE_LABEL: Record<NodePhase, string> = {
  pending: 'pending',
  running: 'running',
  compacting: 'compacting…',
  done: 'done',
  failed: 'failed',
};

function NodePanel({ nodeId, state }: { nodeId: string; state: NodeState }) {
  return (
    <div className="border border-border rounded-md overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-sm px-base py-xs bg-surface-raised border-b border-border">
        <span className="font-mono text-xs text-primary font-semibold">[{nodeId}]</span>
        <span className="text-sm text-text flex-1 truncate">{state.goal}</span>
        <span
          className={`text-xs px-[6px] py-[2px] rounded font-medium ${NODE_PHASE_BADGE[state.phase]}`}
        >
          {NODE_PHASE_LABEL[state.phase]}
        </span>
      </div>

      {/* Tool log */}
      {state.toolLog.length > 0 && (
        <div className="border-b border-border px-base py-xs text-xs font-mono text-text-secondary space-y-[2px]">
          {state.toolLog.map((t, i) => (
            <div key={i} className="flex gap-sm items-baseline">
              <span>{t.done ? '✓' : '⚙'}</span>
              <span className={t.done ? 'text-success' : 'text-primary'}>{t.name}</span>
              <span className="text-text-disabled truncate max-w-[300px]">{t.argsSummary}</span>
              {t.done && t.durationDisplay && (
                <span className="ml-auto shrink-0 text-text-disabled">{t.durationDisplay}</span>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Streamed text */}
      {(state.text || state.error) && (
        <div className="px-base py-sm text-sm text-text whitespace-pre-wrap max-h-[260px] overflow-y-auto">
          {state.error ? (
            <span className="text-danger">{state.error}</span>
          ) : (
            state.text
          )}
        </div>
      )}
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
