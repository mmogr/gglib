/**
 * OrchestratorThread — self-contained lifecycle component for a single run.
 *
 * This is the primary building block for embedding orchestrator runs inside
 * chat messages. For Phase 1 it is self-sufficient: it owns the goal input
 * form, initiates the SSE stream, and manages the full visual lifecycle.
 *
 * State is stored in the OrchestratorRegistry (not the legacy singleton
 * context), so multiple instances can live in the same view without
 * interfering with each other's streams, reducer state, or abort controllers.
 *
 * Visual structure when a run is in-flight or complete:
 *
 *   ┌─ CompactRunCard ──────────────────────── [Show/Hide] ─┐
 *   │  ⟳  Running · 3 / 7 nodes  "Goal text…"              │
 *   └──────────────────────────────────────────────────────-─┘
 *   (expanded)
 *   ┌─ CollapsibleCastingSheet ─────────────────────────────┐
 *   │  Team · 5 roles  [🔍][✍️][🛡️][✏️][👤]               │
 *   └───────────────────────────────────────────────────────┘
 *   ┌─ CollapsibleDagView ──────────────────────────────────┐
 *   │  DAG · 7 nodes  ✓3  ⟳1  ░3  ██████░░░               │
 *   └───────────────────────────────────────────────────────┘
 *   ┌─ Per-node NodePanel (one per graph node) ─────────────┐
 *   │  [auto-open for active/failed/selected nodes]         │
 *   └───────────────────────────────────────────────────────┘
 *   ┌─ Synthesis / Final answer ────────────────────────────┐
 *   └───────────────────────────────────────────────────────┘
 *
 * The HITL approval modal is rendered as a full-screen overlay (using the
 * existing HitlApprovalModal component) for Phase 1 parity with the
 * orchestrator page. It will be replaced with an inline approval card in
 * Phase 4 when threads move into the chat window.
 *
 * Preview / testing:
 *   Add `?thread=1` to the orchestrator page URL to render this component
 *   instead of the full-page layout.
 *
 * @module components/Orchestrator/Thread/OrchestratorThread
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type FC,
  type MutableRefObject,
} from 'react';
import { AlertTriangle, Play, RotateCcw, Square } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Input } from '../../../components/ui/Input';
import { Select } from '../../../components/ui/Select';
import { Icon } from '../../../components/ui/Icon';
import { cn } from '../../../utils/cn';
import {
  createDraftRunId,
  useOrchestratorRun,
  useOrchestratorRegistry,
} from '../../../contexts/OrchestratorRegistry';
import type { ApprovalDecisionPayload } from '../../../types/orchestrator';
import HitlApprovalModal from '../../../pages/Orchestrator/components/HitlApprovalModal';
import NodePanel from '../../../pages/Orchestrator/components/NodePanel';
import CompactRunCard from '../CompactRunCard';
import CollapsibleCastingSheet from '../CollapsibleCastingSheet';
import CollapsibleDagView from '../CollapsibleDagView';
import { useOrchestratorRunStream } from './useOrchestratorRunStream';
import PlanEditor from '../PlanEditor/PlanEditor';

// ─── Constants ────────────────────────────────────────────────────────────────

const HITL_MODE_OPTIONS = [
  { value: 'none', label: 'No approval' },
  { value: 'approve_plan', label: 'Approve plan' },
  { value: 'approve_each_node', label: 'Approve each node' },
  { value: 'approve_tools', label: 'Approve tool calls' },
] as const;

// ─── Props ────────────────────────────────────────────────────────────────────

export interface OrchestratorThreadProps {
  /** llama.cpp HTTP base port. */
  serverPort: number;
  /** Optional model name override forwarded to the orchestrator API. */
  model?: string;
  /** Optional className for the outermost container. */
  className?: string;

  // ── Hosted / composer-embedded mode ────────────────────────────────────────
  /**
   * When provided the thread operates in **hosted mode**:
   * - The internal goal form is hidden (the parent composer drives submission).
   * - The parent calls `startRunRef.current(goal, hitlMode?)` to begin a run.
   * - A plan-kind HITL approval renders as an inline `PlanEditor` rather than
   *   a full-screen modal, so it fits naturally inside the chat thread.
   */
  startRunRef?: MutableRefObject<((goal: string, hitlMode?: string) => void) | null>;
  /**
   * Called once when the run reaches a terminal phase (`complete` or `error`).
   * Receives the current draft run ID and the final answer string (or `null`).
   */
  onRunComplete?: (runId: string, finalAnswer: string | null) => void;
}

// ─── Component ────────────────────────────────────────────────────────────────

/**
 * A fully self-contained orchestrator run thread.
 *
 * Lifecycle:
 *   1. Render idle — shows goal input form.
 *   2. User submits → `startRun(goal, hitlMode)` → SSE stream opens → registry
 *      state updates → CompactRunCard transitions through phases.
 *   3. On HITL gate → `HitlApprovalModal` opens; user approves/rejects.
 *   4. On complete → final answer and synthesis are displayed.
 *   5. "New run" resets local state and generates a fresh run ID.
 */
const OrchestratorThread: FC<OrchestratorThreadProps> = ({
  serverPort,
  model,
  className,
  startRunRef,
  onRunComplete,
}) => {
  // ── Run ID management ──────────────────────────────────────────────────────
  // Initialise once with a draft ID. The draft ID stays for the lifetime of
  // this run slot; it is replaced only when the user clicks "New run".
  const [runId, setRunId] = useState(() => createDraftRunId());

  // ── Registry hooks ─────────────────────────────────────────────────────────
  const { register, unregister } = useOrchestratorRegistry();
  const { session, dispatch, abort, setAbortController, isStreaming } =
    useOrchestratorRun(runId);

  // Stable getter so the approve callback never goes stale between renders.
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const getPendingApproval = useCallback(
    () => sessionRef.current.pendingApproval,
    [],
  );

  // ── Streaming hook ─────────────────────────────────────────────────────────
  const { startRun, cancelRun, approve } = useOrchestratorRunStream({
    dispatch,
    setAbortController,
    abort,
    getPendingApproval,
    serverPort,
    model,
  });

  // ── Registry lifecycle ─────────────────────────────────────────────────────
  useEffect(() => {
    register(runId);
    return () => {
      unregister(runId);
    };
  }, [runId, register, unregister]);

  // ── Local UI state ─────────────────────────────────────────────────────────
  const [goal, setGoal] = useState('');
  const [hitlMode, setHitlMode] = useState<string>('none');
  const [isExpanded, setIsExpanded] = useState(false);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // Whether this thread is operating in composer-hosted mode.
  const isHosted = startRunRef !== undefined;

  // ── Hosted mode: fill the startRunRef with the programmatic start callback ─
  useEffect(() => {
    if (!startRunRef) return;
    startRunRef.current = (goalText: string, hitlModeOverride?: string) => {
      setGoal(goalText);
      setHitlMode(hitlModeOverride ?? 'none');
      void startRun(goalText, hitlModeOverride ?? 'none');
    };
    return () => {
      startRunRef.current = null;
    };
  }, [startRunRef, startRun]);

  // ── Notify parent when the run reaches a terminal phase ────────────────────
  const onRunCompleteRef = useRef(onRunComplete);
  onRunCompleteRef.current = onRunComplete;
  const completionFiredRef = useRef(false);
  useEffect(() => {
    if (
      (session.phase === 'complete' || session.phase === 'error') &&
      !completionFiredRef.current
    ) {
      completionFiredRef.current = true;
      onRunCompleteRef.current?.(runId, session.finalAnswer ?? null);
    }
    if (session.phase === 'idle') {
      completionFiredRef.current = false;
    }
  }, [session.phase, runId, session.finalAnswer]);

  // Auto-expand as soon as the run goes active.
  const prevPhaseRef = useRef(session.phase);
  useEffect(() => {
    const prev = prevPhaseRef.current;
    prevPhaseRef.current = session.phase;
    if (
      prev === 'idle' &&
      (session.phase === 'planning' ||
        session.phase === 'running' ||
        session.phase === 'synthesizing')
    ) {
      setIsExpanded(true);
    }
  }, [session.phase]);

  // ── Derived state ──────────────────────────────────────────────────────────
  const isActive =
    session.phase === 'running' ||
    session.phase === 'planning' ||
    session.phase === 'synthesizing' ||
    session.phase === 'awaiting_approval';

  const isTerminal =
    session.phase === 'complete' || session.phase === 'error';

  const nodeIds = session.graph ? Object.keys(session.graph.nodes) : [];

  // ── Handlers ──────────────────────────────────────────────────────────────
  const handleSubmit = useCallback(
    async (e: FormEvent) => {
      e.preventDefault();
      if (!goal.trim() || isActive) return;
      await startRun(goal.trim(), hitlMode);
    },
    [goal, hitlMode, isActive, startRun],
  );

  const handleNewRun = useCallback(() => {
    // Abort any live stream, unregister the old slot, and generate a fresh ID.
    cancelRun();
    unregister(runId);
    const nextId = createDraftRunId();
    setRunId(nextId);
    register(nextId);
    setGoal('');
    setHitlMode('none');
    setIsExpanded(false);
    setSelectedNodeId(null);
  }, [cancelRun, unregister, runId, register]);

  const handleApprove = useCallback(
    (payload: ApprovalDecisionPayload) => {
      void approve(payload);
    },
    [approve],
  );

  const handleReject = useCallback(
    (reason?: string) => {
      void approve({ decision: 'reject', reason });
    },
    [approve],
  );

  const handleToggleNode = useCallback((id: string) => {
    setSelectedNodeId((prev) => (prev === id ? null : id));
  }, []);

  // ── Render ─────────────────────────────────────────────────────────────────
  // Determine whether a plan approval is pending so we can render the
  // PlanEditor inline instead of falling through to HitlApprovalModal.
  const isPlanApproval =
    session.pendingApproval?.kind.kind === 'plan' && session.graph !== null;

  return (
    <div
      className={cn('flex flex-col gap-sm', className)}
      data-testid="orchestrator-thread"
      data-run-id={runId}
      data-phase={session.phase}
    >
      {/* ── Idle: goal input form (hidden in hosted mode) ───────────────────── */}
      {session.phase === 'idle' && !isHosted && (
        <form onSubmit={handleSubmit} className="flex flex-col gap-sm">
          <div className="flex gap-sm">
            <Input
              value={goal}
              onChange={(e) => setGoal(e.target.value)}
              placeholder="Describe the goal for the orchestrator…"
              className="flex-1"
              data-testid="thread-goal-input"
            />
            <Button
              type="submit"
              variant="primary"
              size="md"
              disabled={!goal.trim()}
              data-testid="thread-run-button"
            >
              <Icon icon={Play} size={14} />
              Run
            </Button>
          </div>

          <div className="flex items-center gap-sm">
            <label className="text-sm text-text-secondary shrink-0">Approval:</label>
            <Select
              value={hitlMode}
              onChange={(e) => setHitlMode(e.target.value)}
              size="sm"
            >
              {HITL_MODE_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </div>
        </form>
      )}

      {/* ── Active / terminal: compact status header ───────────────────────── */}
      {session.phase !== 'idle' && (
        <CompactRunCard
          goal={goal || session.graph?.goal || ''}
          phase={session.phase}
          graph={session.graph}
          nodeStates={session.nodeStates}
          isExpanded={isExpanded}
          onToggle={() => setIsExpanded((v) => !v)}
        />
      )}

      {/* ── Cancel button (shown alongside the chip while active) ──────────── */}
      {isActive && isStreaming && (
        <div className="flex justify-end">
          <Button
            type="button"
            variant="danger"
            size="sm"
            onClick={cancelRun}
            data-testid="thread-cancel-button"
          >
            <Icon icon={Square} size={12} />
            Cancel
          </Button>
        </div>
      )}

      {/* ── Expanded detail panel ─────────────────────────────────────────── */}
      {session.phase !== 'idle' && isExpanded && (
        <div className="flex flex-col gap-sm pl-sm border-l-2 border-border ml-xs">
          {/* Error banner */}
          {session.error && (
            <div
              className="rounded-base border border-danger/30 bg-danger/10 px-sm py-xs flex items-start gap-xs"
              role="alert"
              data-testid="thread-error-banner"
            >
              <Icon icon={AlertTriangle} size={13} className="text-danger shrink-0 mt-[1px]" />
              <p className="text-xs text-danger">{session.error}</p>
            </div>
          )}

          {/* CollapsibleCastingSheet — once the graph is known */}
          {session.graph && (
            <CollapsibleCastingSheet
              graph={session.graph}
              nodeStates={session.nodeStates}
              selectedNodeId={selectedNodeId}
              onSelectNode={handleToggleNode}
            />
          )}

          {/* CollapsibleDagView */}
          {session.graph && (
            <CollapsibleDagView
              graph={session.graph}
              nodeStates={session.nodeStates}
              selectedNodeId={selectedNodeId}
              onSelectNode={handleToggleNode}
              runId={runId}
            />
          )}

          {/* Per-node panels */}
          {nodeIds.length > 0 && session.graph && (
            <div
              className="flex flex-col gap-xs"
              data-testid="thread-node-panels"
            >
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
                  graph={session.graph ?? undefined}
                  port={serverPort}
                  model={model}
                  runId={runId}
                  isActive={isActive}
                />
              ))}
            </div>
          )}

          {/* Synthesis streaming text */}
          {(session.synthesisText || session.phase === 'synthesizing') && (
            <div
              className="flex flex-col gap-xs"
              data-testid="thread-synthesis"
            >
              <p className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                Synthesis
              </p>
              <div className="rounded-base border border-border bg-surface px-sm py-xs">
                <pre className="text-sm text-text whitespace-pre-wrap font-sans leading-relaxed">
                  {session.synthesisText}
                  {session.phase === 'synthesizing' && (
                    <span className="inline-block w-[2px] h-[1em] bg-primary align-text-bottom animate-blink ml-[1px]" />
                  )}
                </pre>
              </div>
            </div>
          )}

          {/* Final answer */}
          {session.phase === 'complete' && session.finalAnswer && (
            <div
              className="flex flex-col gap-xs"
              data-testid="thread-final-answer"
            >
              <p className="text-xs font-semibold text-success uppercase tracking-wide">
                Answer
              </p>
              <div className="rounded-base border border-success/30 bg-success/5 px-sm py-xs">
                <pre className="text-sm text-text whitespace-pre-wrap font-sans leading-relaxed">
                  {session.finalAnswer}
                </pre>
              </div>
            </div>
          )}
        </div>
      )}

      {/* ── Terminal: "New run" reset button (hidden in hosted mode) ─────── */}
      {isTerminal && !isHosted && (
        <div className="flex">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={handleNewRun}
            data-testid="thread-new-run-button"
          >
            <Icon icon={RotateCcw} size={12} />
            New run
          </Button>
        </div>
      )}

      {/* ── Plan approval: inline PlanEditor ──────────────────────────────── */}
      {isPlanApproval && session.graph && (
        <PlanEditor
          graph={session.graph}
          onApprove={handleApprove}
          onReject={handleReject}
          costEstimate={session.costEstimate}
          submitting={session.pendingApproval!.submitting}
          className="mt-sm rounded-base border border-border overflow-hidden"
        />
      )}

      {/* ── HITL approval modal (non-plan kinds only) ──────────────────────── */}
      {session.pendingApproval && !isPlanApproval && (
        <HitlApprovalModal
          open={true}
          kind={session.pendingApproval.kind}
          graph={session.graph}
          submitting={session.pendingApproval.submitting}
          costEstimate={session.costEstimate}
          port={serverPort}
          model={model}
          onApprove={handleApprove}
          onReject={handleReject}
        />
      )}
    </div>
  );
};

export default OrchestratorThread;
