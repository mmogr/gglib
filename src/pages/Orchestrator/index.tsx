/**
 * Orchestrator page — Phase F native frontend.
 *
 * Provides:
 *   - Goal input form + run/cancel buttons
 *   - DAG visualization with clickable nodes
 *   - Per-node collapsible NodePanel components
 *   - Synthesis streaming output
 *   - HITL approval modals (plan / node / tool)
 *   - Resumable runs sidebar via RunsList
 *
 * Uses the OrchestratorContext + useOrchestrator hook.
 * HTTP transport only — no tauri::command, no isTauriApp branching.
 */

import { useState, useEffect, useCallback } from 'react';
import { ArrowLeft, Play, Square, RotateCcw, ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Icon } from '../../components/ui/Icon';
import { useOrchestrator } from '../../hooks/useOrchestrator';
import { useSettingsContext } from '../../contexts/SettingsContext';
import type { OrchestratorRun } from '../../types/orchestrator';
import DagView from './components/DagView';
import NodePanel from './components/NodePanel';
import HitlApprovalModal from './components/HitlApprovalModal';
import RunsList from './components/RunsList';

const HITL_MODE_OPTIONS = [
  { value: 'none', label: 'No approval' },
  { value: 'approve_plan', label: 'Approve plan' },
  { value: 'approve_each_node', label: 'Approve each node' },
  { value: 'approve_tools', label: 'Approve tool calls' },
];

interface OrchestratorPageProps {
  onBack?: () => void;
}

export default function OrchestratorPage({ onBack }: OrchestratorPageProps) {
  const { settings } = useSettingsContext();
  const serverPort = settings?.llamaBasePort ?? 9000;

  const { session, run, resume, cancel, reset, approve, loadRuns, isStreaming } = useOrchestrator({
    serverPort,
  });

  const [goal, setGoal] = useState('');
  const [hitlMode, setHitlMode] = useState('none');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [showRuns, setShowRuns] = useState(false);
  const [showNodes, setShowNodes] = useState(true);

  const isActive = session.phase === 'running' || session.phase === 'awaiting_approval' || session.phase === 'synthesizing';

  // Load runs on mount
  useEffect(() => {
    loadRuns();
  }, [loadRuns]);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!goal.trim() || isActive) return;
      await run(goal.trim(), hitlMode);
    },
    [goal, hitlMode, isActive, run],
  );

  const handleResume = useCallback(
    async (r: OrchestratorRun) => {
      setGoal(r.goal);
      setShowRuns(false);
      await resume(r.id);
    },
    [resume],
  );

  const handleApproveWithPayload = useCallback(
    (payload: Parameters<typeof approve>[0]) => {
      approve(payload);
    },
    [approve],
  );

  const handleReject = useCallback(
    (reason?: string) => {
      approve({ decision: 'reject', reason });
    },
    [approve],
  );

  // Auto-open nodes section when run starts
  useEffect(() => {
    if (session.phase === 'running') setShowNodes(true);
  }, [session.phase]);

  const nodeIds = session.graph ? Object.keys(session.graph.nodes) : [];

  return (
    <div className="flex h-full overflow-hidden bg-background">
      {/* ── Sidebar ────────────────────────────────────────────────────── */}
      <aside className="hidden md:flex flex-col w-[280px] shrink-0 border-r border-border bg-surface overflow-y-auto p-md gap-md scrollbar-thin">
        {onBack && (
          <Button variant="ghost" size="sm" onClick={onBack} className="self-start -ml-xs">
            <Icon icon={ArrowLeft} size={14} />
            Back
          </Button>
        )}

        <div>
          <p className="text-xs font-semibold text-text-muted uppercase tracking-wide mb-sm">Orchestrator</p>
          <p className="text-xs text-text-secondary leading-relaxed">
            Multi-node task execution with a director planner and parallel workers.
          </p>
        </div>

        {/* Runs toggle */}
        <button
          className="flex items-center gap-xs text-sm font-medium text-text bg-transparent border-none cursor-pointer p-0 hover:text-primary transition-colors"
          onClick={() => setShowRuns((v) => !v)}
        >
          <Icon icon={showRuns ? ChevronDown : ChevronRight} size={13} />
          Previous runs
        </button>

        {showRuns && (
          <RunsList
            runs={session.runs}
            loading={session.runsLoading}
            onRefresh={() => loadRuns()}
            onSelectRun={handleResume}
          />
        )}
      </aside>

      {/* ── Main content ───────────────────────────────────────────────── */}
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Header */}
        <div className="flex items-center gap-sm px-md py-sm border-b border-border shrink-0">
          {onBack && (
            <Button variant="ghost" size="sm" onClick={onBack} className="md:hidden -ml-xs" iconOnly title="Back">
              <Icon icon={ArrowLeft} size={14} />
            </Button>
          )}
          <h1 className="text-lg font-semibold text-text flex-1">Orchestrator</h1>
          {(session.phase === 'complete' || session.phase === 'error') && (
            <Button variant="secondary" size="sm" onClick={reset}>
              <Icon icon={RotateCcw} size={13} />
              New run
            </Button>
          )}
        </div>

        {/* Scrollable body */}
        <div className="flex-1 overflow-y-auto p-md flex flex-col gap-lg scrollbar-thin">
          {/* Goal input form */}
          <form onSubmit={handleSubmit} className="flex flex-col gap-sm">
            <div className="flex gap-sm">
              <Input
                value={goal}
                onChange={(e) => setGoal(e.target.value)}
                placeholder="Describe the goal for the orchestrator…"
                disabled={isActive}
                className="flex-1"
              />
              {isActive ? (
                <Button type="button" variant="danger" size="md" onClick={cancel} iconOnly title="Cancel">
                  <Icon icon={Square} size={14} />
                </Button>
              ) : (
                <Button type="submit" variant="primary" size="md" disabled={!goal.trim()}>
                  <Icon icon={Play} size={14} />
                  Run
                </Button>
              )}
            </div>

            <div className="flex items-center gap-sm">
              <label className="text-sm text-text-secondary shrink-0">HITL mode:</label>
              <Select
                value={hitlMode}
                onChange={(e) => setHitlMode(e.target.value)}
                disabled={isActive}
                size="sm"
              >
                {HITL_MODE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </Select>
            </div>
          </form>

          {/* Error banner */}
          {session.error && (
            <div className="rounded-base border border-danger/30 bg-danger/10 px-md py-sm">
              <p className="text-sm text-danger">{session.error}</p>
            </div>
          )}

          {/* Phase indicator */}
          {isActive && (
            <div className="flex items-center gap-sm text-sm text-text-secondary">
              <span className="inline-block w-2 h-2 rounded-full bg-primary animate-pulse" />
              {session.phase === 'synthesizing'
                ? 'Synthesizing final answer…'
                : session.phase === 'awaiting_approval'
                  ? 'Waiting for your approval…'
                  : 'Running…'}
            </div>
          )}

          {/* DAG visualization */}
          {session.graph && (
            <section className="flex flex-col gap-sm">
              <button
                className="flex items-center gap-xs text-sm font-semibold text-text bg-transparent border-none cursor-pointer p-0 hover:text-primary transition-colors"
                onClick={() => setShowNodes((v) => !v)}
              >
                <Icon icon={showNodes ? ChevronDown : ChevronRight} size={14} />
                Task graph
                <span className="ml-xs text-xs text-text-muted font-normal">
                  ({nodeIds.length} nodes)
                </span>
              </button>

              {showNodes && (
                <>
                  <DagView
                    graph={session.graph}
                    nodeStates={session.nodeStates}
                    selectedNodeId={selectedNodeId}
                    onSelectNode={(id) => setSelectedNodeId((prev) => (prev === id ? null : id))}
                  />

                  {/* Node panels */}
                  {nodeIds.length > 0 && (
                    <div className="flex flex-col gap-xs mt-xs">
                      {nodeIds.map((id) => (
                        <NodePanel
                          key={id}
                          nodeId={id}
                          node={session.graph!.nodes[id]}
                          nodeState={session.nodeStates[id]}
                          defaultOpen={
                            id === selectedNodeId ||
                            session.nodeStates[id]?.phase === 'running' ||
                            session.nodeStates[id]?.phase === 'failed'
                          }
                        />
                      ))}
                    </div>
                  )}
                </>
              )}
            </section>
          )}

          {/* Synthesis streaming output */}
          {(session.synthesisText || session.phase === 'synthesizing') && (
            <section className="flex flex-col gap-sm">
              <h2 className="text-sm font-semibold text-text">Synthesis</h2>
              <div className="rounded-base border border-border bg-surface p-md">
                <pre className="text-sm text-text whitespace-pre-wrap font-sans leading-relaxed">
                  {session.synthesisText}
                  {session.phase === 'synthesizing' && (
                    <span className="inline-block w-[2px] h-[1em] bg-primary align-text-bottom animate-blink ml-[1px]" />
                  )}
                </pre>
              </div>
            </section>
          )}

          {/* Final answer */}
          {session.phase === 'complete' && session.finalAnswer && (
            <section className="flex flex-col gap-sm">
              <h2 className="text-sm font-semibold text-success">Answer</h2>
              <div className="rounded-base border border-success/30 bg-success/5 p-md">
                <pre className="text-sm text-text whitespace-pre-wrap font-sans leading-relaxed">
                  {session.finalAnswer}
                </pre>
              </div>
            </section>
          )}

          {/* Mobile: previous runs */}
          <section className="flex flex-col gap-sm md:hidden">
            <button
              className="flex items-center gap-xs text-sm font-semibold text-text bg-transparent border-none cursor-pointer p-0 hover:text-primary transition-colors"
              onClick={() => setShowRuns((v) => !v)}
            >
              <Icon icon={showRuns ? ChevronDown : ChevronRight} size={14} />
              Previous runs
            </button>

            {showRuns && (
              <RunsList
                runs={session.runs}
                loading={session.runsLoading}
                onRefresh={() => loadRuns()}
                onSelectRun={handleResume}
              />
            )}
          </section>
        </div>
      </main>

      {/* HITL approval modal */}
      {session.pendingApproval && (
        <HitlApprovalModal
          open={true}
          kind={session.pendingApproval.kind}
          graph={session.graph}
          submitting={session.pendingApproval.submitting}
          onApprove={handleApproveWithPayload}
          onReject={handleReject}
        />
      )}

      {/* suppress unused warning */}
      {void isStreaming}
    </div>
  );
}
