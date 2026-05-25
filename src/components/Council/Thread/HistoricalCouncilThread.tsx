/**
 * HistoricalCouncilThread — read-only view of a completed run.
 *
 * Fetches a run by ID from `GET /api/council/runs/{id}`, replays its
 * stored events through `orchestratorReducer` to reconstruct the terminal
 * session state, seeds the CouncilRegistry with that state, then renders
 * the familiar CompactRunCard + collapsed-by-default detail layout.
 *
 * Since the run is completed (or interrupted), there is no SSE stream,
 * no approval UI, and no controls — only the read-only final state.
 *
 * Loading states:
 *   fetching  — skeleton placeholder
 *   error     — error callout with retry button
 *   ready     — full collapsed layout

 * @module components/Council/Thread/HistoricalCouncilThread
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
  useCouncilRun,
  useCouncilRegistry,
} from '../../../contexts/CouncilRegistry';
import {
  orchestratorReducer,
  type CouncilSession,
} from '../../../contexts/CouncilContext';
import { councilEventToAction } from '../../../utils/councilEventToAction';
import {
  getCouncilRun,
} from '../../../services/clients/council';
import type { CouncilEvent } from '../../../types/council';
import NodePanel from '../../../pages/Council/components/NodePanel';
import CompactRunCard from '../CompactRunCard';
import CollapsibleCastingSheet from '../CollapsibleCastingSheet';
import CollapsibleDagView from '../CollapsibleDagView';

// ─── Event replay ───────────────────────────────────────────────

/**
 * Replay a list of serialised run events through the orchestrator reducer to
 * reconstruct the final session state.
 *
 * The `event_json` field of each `OrchestratorRunEvent` is a JSON string of a
 * full `CouncilEvent` — we parse and translate each one in sequence.
 */
function replayEvents(eventJsonStrings: string[]): CouncilSession {
  let session: CouncilSession = {
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
    let event: CouncilEvent;
    try {
      event = JSON.parse(raw) as CouncilEvent;
    } catch {
      continue; // skip malformed stored events
    }
    const action = councilEventToAction(event);
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

export interface HistoricalCouncilThreadProps {
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
 *  3. Replays events → builds `CouncilSession` → seeds the registry.
 *  4. `useCouncilRun(runId)` then subscribes to the seeded state.
 *  5. On unmount: unregisters to free registry memory.
 *
 * If the same `runId` is shown more than once in the same view (e.g. two
 * bubbles referencing the same run), registry deduplication ensures only one
 * copy of the state exists.
 */
const HistoricalCouncilThread: FC<HistoricalCouncilThreadProps> = ({
  runId,
  defaultExpanded = false,
  className,
}) => {
  const skeletonId = useId();
  const { register, seed, unregister, hasRun } = useCouncilRegistry();
  const { session } = useCouncilRun(runId);

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
      const { events } = await getCouncilRun(runId);
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

export default HistoricalCouncilThread;
