/**
 * HistoricalOrchestratorThread — read-only view of a completed run.
 *
 * Fetches a run by ID from `GET /api/orchestrator/runs/{id}`, replays its
 * stored events through `orchestratorReducer` to reconstruct the terminal
 * session state, seeds the OrchestratorRegistry with that state, then renders
 * the familiar CompactRunCard + collapsed-by-default detail layout.
 *
 * Since the run is completed (or interrupted), there is no SSE stream,
 * no approval UI, and no controls — only the read-only final state.
 *
 * Loading states:
 *   fetching  — skeleton placeholder
 *   error     — error callout with retry button
 *   ready     — full collapsed layout

 * @module components/Orchestrator/Thread/HistoricalOrchestratorThread
 */

import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FC,
} from 'react';
import { AlertTriangle, RefreshCcw } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Icon } from '../../../components/ui/Icon';
import { cn } from '../../../utils/cn';
import {
  useOrchestratorRun,
  useOrchestratorRegistry,
} from '../../../contexts/OrchestratorRegistry';
import {
  orchestratorReducer,
  type OrchestratorSession,
} from '../../../contexts/OrchestratorContext';
import {
  getOrchestratorRun,
} from '../../../services/clients/orchestrator';
import type { OrchestratorEvent } from '../../../types/orchestrator';
import NodePanel from '../../../pages/Orchestrator/components/NodePanel';
import CompactRunCard from '../CompactRunCard';
import CollapsibleCastingSheet from '../CollapsibleCastingSheet';
import CollapsibleDagView from '../CollapsibleDagView';

// ─── Event replay ─────────────────────────────────────────────────────────────

/**
 * Map a raw persisted OrchestratorEvent to an OrchestratorAction.
 * Returns null for purely informational events that have no reducer effect.
 *
 * This is the same translation used by `useOrchestratorRunStream` — copied
 * here to keep this file self-contained (no cross-module dependency on the
 * streaming hook; it will be factored into a shared utility in Phase 7).
 */
import type { OrchestratorAction } from '../../../contexts/OrchestratorContext';

function eventToAction(event: OrchestratorEvent): OrchestratorAction | null {
  switch (event.type) {
    case 'plan_proposed':
      return { type: 'PLAN_PROPOSED', graph: event.graph };
    case 'run_cost_estimate':
      return {
        type: 'SET_COST_ESTIMATE',
        nodeCount: event.node_count,
        estTokens: event.est_tokens,
        estWallSeconds: event.est_wall_seconds,
      };
    case 'plan_approved':
      return { type: 'PLAN_APPROVED' };
    case 'plan_rejected':
      return { type: 'PLAN_REJECTED', reason: event.reason };
    case 'replan_attempt':
      return { type: 'REPLAN_ATTEMPT', attempt: event.attempt, reason: event.reason };
    case 'awaiting_approval':
      return { type: 'AWAITING_APPROVAL', approvalId: event.approval_id, kind: event.kind };
    case 'node_started':
      return { type: 'NODE_STARTED', nodeId: event.node_id, goal: event.goal };
    case 'node_text_delta':
      return { type: 'NODE_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'node_tool_call_start':
      return {
        type: 'NODE_TOOL_CALL_START',
        nodeId: event.node_id,
        displayName: event.display_name,
        argsSummary: event.args_summary,
      };
    case 'node_tool_call_complete':
      return {
        type: 'NODE_TOOL_CALL_COMPLETE',
        nodeId: event.node_id,
        toolName: event.tool_name,
        displayName: event.display_name,
        durationDisplay: event.duration_display,
      };
    case 'node_compacting':
      return { type: 'NODE_COMPACTING', nodeId: event.node_id };
    case 'node_complete':
      return { type: 'NODE_COMPLETE', nodeId: event.node_id, outputPreview: event.output_preview };
    case 'node_failed':
      return { type: 'NODE_FAILED', nodeId: event.node_id, error: event.error };
    case 'synthesis_start':
      return { type: 'SYNTHESIS_START' };
    case 'synthesis_text_delta':
      return { type: 'SYNTHESIS_TEXT_DELTA', delta: event.delta };
    case 'synthesis_complete':
      return { type: 'SYNTHESIS_COMPLETE', content: event.content };
    case 'orchestrator_complete':
      return { type: 'ORCHESTRATOR_COMPLETE', answer: event.answer };
    case 'orchestrator_error':
      return { type: 'ORCHESTRATOR_ERROR', message: event.message };
    case 'steering_applied':
      return { type: 'SET_PENDING_DIFF', diff: event.diff };
    default:
      return null;
  }
}

/**
 * Replay a list of serialised run events through the orchestrator reducer to
 * reconstruct the final session state.
 *
 * The `event_json` field of each `OrchestratorRunEvent` is a JSON string of a
 * full `OrchestratorEvent` — we parse and translate each one in sequence.
 */
function replayEvents(eventJsonStrings: string[]): OrchestratorSession {
  let session: OrchestratorSession = {
    phase: 'idle',
    graph: null,
    nodeStates: {},
    synthesisText: '',
    finalAnswer: null,
    pendingApproval: null,
    costEstimate: null,
    error: null,
    pendingDiff: null,
    runs: [],
    runsLoading: false,
  };

  for (const raw of eventJsonStrings) {
    let event: OrchestratorEvent;
    try {
      event = JSON.parse(raw) as OrchestratorEvent;
    } catch {
      continue; // skip malformed stored events
    }
    const action = eventToAction(event);
    if (action) {
      session = orchestratorReducer(session, action);
    }
  }

  return session;
}

// ─── Fetch state machine ──────────────────────────────────────────────────────

type FetchState =
  | { status: 'idle' }
  | { status: 'fetching' }
  | { status: 'error'; message: string }
  | { status: 'ready' };

// ─── Props ────────────────────────────────────────────────────────────────────

export interface HistoricalOrchestratorThreadProps {
  /** Server-assigned run ID to load and replay. */
  runId: string;
  /** When true, expand the detail panel immediately without the user clicking Show. */
  defaultExpanded?: boolean;
  /** Optional className for the outermost container. */
  className?: string;
}

// ─── Skeleton ─────────────────────────────────────────────────────────────────

const ThreadSkeleton: FC<{ id: string }> = ({ id }) => (
  <div
    className="flex items-center gap-sm px-sm py-xs rounded-base border border-border bg-surface animate-pulse"
    aria-busy="true"
    aria-label="Loading run…"
    data-testid="historical-thread-skeleton"
    id={id}
  >
    <span className="w-3 h-3 rounded-full bg-border shrink-0" />
    <span className="h-[10px] w-24 rounded-sm bg-border" />
    <span className="h-[10px] flex-1 rounded-sm bg-border max-w-[180px]" />
  </div>
);

// ─── Component ────────────────────────────────────────────────────────────────

/**
 * Loads a completed run's event log, replays it to reconstruct full session
 * state, and renders a collapsed-by-default read-only thread view.
 *
 * The component:
 *  1. On mount: registers the `runId` in the registry (no-op if already there).
 *  2. Fetches the run + events from the API.
 *  3. Replays events → builds `OrchestratorSession` → seeds the registry.
 *  4. `useOrchestratorRun(runId)` then subscribes to the seeded state.
 *  5. On unmount: unregisters to free registry memory.
 *
 * If the same `runId` is shown more than once in the same view (e.g. two
 * bubbles referencing the same run), registry deduplication ensures only one
 * copy of the state exists.
 */
const HistoricalOrchestratorThread: FC<HistoricalOrchestratorThreadProps> = ({
  runId,
  defaultExpanded = false,
  className,
}) => {
  const skeletonId = useId();
  const { register, seed, unregister, hasRun } = useOrchestratorRegistry();
  const { session } = useOrchestratorRun(runId);

  const [fetchState, setFetchState] = useState<FetchState>({ status: 'idle' });
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // Guard: track whether the fetch has already been attempted for this runId
  // to avoid duplicate fetches on StrictMode double-mount.
  const fetchedRef = useRef(false);

  const loadRun = useCallback(async () => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;

    setFetchState({ status: 'fetching' });
    try {
      const { events } = await getOrchestratorRun(runId);
      const seededSession = replayEvents(events.map((e) => e.event_json));
      seed(runId, seededSession);
      setFetchState({ status: 'ready' });
    } catch (err: unknown) {
      const message =
        err instanceof Error ? err.message : 'Failed to load run';
      setFetchState({ status: 'error', message });
    }
  }, [runId, seed]);

  useEffect(() => {
    register(runId);
    // If the run is already seeded (e.g. this component remounts for a run
    // that was previously loaded in the same session), skip the fetch.
    if (hasRun(runId) && session.phase !== 'idle') {
      fetchedRef.current = true;
      setFetchState({ status: 'ready' });
    } else {
      void loadRun();
    }
    return () => {
      unregister(runId);
    };
  }, [runId, register, unregister, hasRun, session.phase, loadRun]);

  const handleToggleNode = useCallback((id: string) => {
    setSelectedNodeId((prev) => (prev === id ? null : id));
  }, []);

  const nodeIds = session.graph ? Object.keys(session.graph.nodes) : [];

  // ── Fetching ───────────────────────────────────────────────────────────────
  if (fetchState.status === 'idle' || fetchState.status === 'fetching') {
    return <ThreadSkeleton id={skeletonId} />;
  }

  // ── Error ──────────────────────────────────────────────────────────────────
  if (fetchState.status === 'error') {
    return (
      <div
        className={cn(
          'flex items-start gap-xs px-sm py-xs rounded-base border border-danger/30 bg-danger/10',
          className,
        )}
        role="alert"
        data-testid="historical-thread-error"
      >
        <Icon
          icon={AlertTriangle}
          size={13}
          className="text-danger shrink-0 mt-[1px]"
        />
        <span className="text-xs text-danger flex-1 min-w-0 truncate">
          {fetchState.message}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => {
            fetchedRef.current = false;
            void loadRun();
          }}
          data-testid="historical-thread-retry"
        >
          <Icon icon={RefreshCcw} size={11} />
          Retry
        </Button>
      </div>
    );
  }

  // ── Ready: render seeded session ───────────────────────────────────────────
  return (
    <div
      className={cn('flex flex-col gap-sm', className)}
      data-testid="historical-orchestrator-thread"
      data-run-id={runId}
      data-phase={session.phase}
    >
      {/* Status chip — read-only toggle */}
      <CompactRunCard
        goal={session.graph?.goal ?? runId}
        phase={session.phase}
        graph={session.graph}
        nodeStates={session.nodeStates}
        isExpanded={isExpanded}
        onToggle={() => setIsExpanded((v) => !v)}
      />

      {/* Expanded detail — collapsed by default */}
      {isExpanded && (
        <div className="flex flex-col gap-sm pl-sm border-l-2 border-border ml-xs">
          {/* Error stored in final session */}
          {session.error && (
            <div
              className="rounded-base border border-danger/30 bg-danger/10 px-sm py-xs flex items-start gap-xs"
              role="alert"
            >
              <Icon icon={AlertTriangle} size={13} className="text-danger shrink-0 mt-[1px]" />
              <p className="text-xs text-danger">{session.error}</p>
            </div>
          )}

          {/* Team / casting sheet */}
          {session.graph && (
            <CollapsibleCastingSheet
              graph={session.graph}
              nodeStates={session.nodeStates}
              selectedNodeId={selectedNodeId}
              onSelectNode={handleToggleNode}
            />
          )}

          {/* DAG */}
          {session.graph && (
            <CollapsibleDagView
              graph={session.graph}
              nodeStates={session.nodeStates}
              selectedNodeId={selectedNodeId}
              onSelectNode={handleToggleNode}
              runId={runId}
            />
          )}

          {/* Node panels — read-only (isActive=false, no runId for steering) */}
          {nodeIds.length > 0 && session.graph && (
            <div
              className="flex flex-col gap-xs"
              data-testid="historical-thread-node-panels"
            >
              {nodeIds.map((id) => (
                <NodePanel
                  key={id}
                  nodeId={id}
                  node={session.graph!.nodes[id]}
                  nodeState={session.nodeStates[id]}
                  defaultOpen={
                    id === selectedNodeId ||
                    session.nodeStates[id]?.phase === 'failed'
                  }
                  graph={session.graph ?? undefined}
                  isActive={false}
                />
              ))}
            </div>
          )}

          {/* Synthesis */}
          {session.synthesisText && (
            <div className="flex flex-col gap-xs" data-testid="historical-thread-synthesis">
              <p className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                Synthesis
              </p>
              <div className="rounded-base border border-border bg-surface px-sm py-xs">
                <pre className="text-sm text-text whitespace-pre-wrap font-sans leading-relaxed">
                  {session.synthesisText}
                </pre>
              </div>
            </div>
          )}

          {/* Final answer */}
          {session.finalAnswer && (
            <div className="flex flex-col gap-xs" data-testid="historical-thread-answer">
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
    </div>
  );
};

export default HistoricalOrchestratorThread;
