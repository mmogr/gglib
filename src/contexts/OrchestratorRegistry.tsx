/**
 * OrchestratorRegistry — per-run context isolation.
 *
 * Replaces the page-singleton OrchestratorProvider with a registry pattern.
 * A single OrchestratorRegistryProvider mounts at the app root and manages a
 * Map<runId, OrchestratorSession> so multiple OrchestratorThread instances
 * (live or historical) can coexist in a chat view without shared state or
 * shared SSE streams.
 *
 * Key design properties:
 *
 * 1. FINE-GRAINED RE-RENDERS — useSyncExternalStore subscribes each consumer
 *    to exactly one run's notifications. Dispatching to run A never causes
 *    subscribers of run B to re-render.
 *
 * 2. SSE ISOLATION — each run owns its AbortController. Aborting run A does
 *    not affect run B. Calling setAbortController() replaces and aborts any
 *    previously attached controller for that run.
 *
 * 3. DRAFT → REAL PROMOTION — before the server returns a run id, callers
 *    create a client-side draft id via createDraftRunId(), register it, open
 *    the SSE stream, then call promoteRun(draftId, realId) as soon as the
 *    server responds. The component switches its local state to the real id.
 *
 * 4. STABLE SENTINEL — useOrchestratorRun on an unregistered id returns a
 *    frozen UNREGISTERED_SESSION constant (not a new object per call), so
 *    useSyncExternalStore's reference-equality bail-out works correctly and
 *    does not loop.
 *
 * 5. NO EXISTING FILES MODIFIED — this is purely additive during Phase 1.
 *    The legacy OrchestratorProvider and useOrchestratorContext remain intact.
 *
 * Exported hooks:
 *   useOrchestratorRun(runId)   — state + dispatch for a single run
 *   useOrchestratorRegistry()  — lifecycle: register / seed / promote / unregister
 *   useOrchestratorRunsList()  — global runs list, decoupled from per-run state
 *
 * Exported utilities:
 *   createDraftRunId()   — generate a client-side id before the server responds
 *   isDraftRunId(id)     — true for ids created by createDraftRunId()
 *
 * @module contexts/OrchestratorRegistry
 */

import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useSyncExternalStore,
  type Dispatch,
  type ReactNode,
} from 'react';

import {
  orchestratorReducer,
  type OrchestratorAction,
  type OrchestratorSession,
} from './OrchestratorContext';

import type { OrchestratorRun } from '../types/orchestrator';

// ─── Draft run utilities ──────────────────────────────────────────────────────

const DRAFT_PREFIX = 'draft:' as const;

/**
 * Create a temporary client-side run id for use before the server returns the
 * real run id. Pass it to register(), then call promoteRun(draftId, realId)
 * once the server responds.
 */
export function createDraftRunId(): string {
  return `${DRAFT_PREFIX}${crypto.randomUUID()}`;
}

/** True if `runId` was created by createDraftRunId() and has not been promoted. */
export function isDraftRunId(runId: string): boolean {
  return runId.startsWith(DRAFT_PREFIX);
}

// ─── Empty session factory ────────────────────────────────────────────────────

// createEmptySession is not exported from OrchestratorContext, so we
// replicate the initial shape here. The `runs` / `runsLoading` fields from
// the legacy page-singleton shape are carried as empty defaults to keep the
// existing reducer compatible; the registry manages the runs list separately.
function createPerRunEmptySession(): OrchestratorSession {
  return {
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
}

// ─── Stable sentinel for unregistered runs ────────────────────────────────────

/**
 * Returned by getSession() for any id that is not registered.
 *
 * This must be a stable (===) reference so that useSyncExternalStore does not
 * trigger a re-render when the same unregistered id is queried on consecutive
 * renders. orchestratorReducer always returns a new object on state change, so
 * any registered session stored in the Map is a different reference after each
 * dispatch — the bail-out check works correctly for both cases.
 */
const UNREGISTERED_SESSION: OrchestratorSession = Object.freeze(
  createPerRunEmptySession(),
) as OrchestratorSession;

// ─── Registry class ───────────────────────────────────────────────────────────

/**
 * Plain-class store for all live and historical orchestrator sessions.
 *
 * One instance lives for the app's lifetime, held in a useRef inside
 * OrchestratorRegistryProvider. It is never recreated on re-renders.
 * All mutations are synchronous; subscribers are notified immediately after
 * each mutation so useSyncExternalStore receives the new value on the next
 * microtask tick.
 */
class OrchestratorRegistry {
  // ── Per-run state ────────────────────────────────────────────────────────

  private readonly sessions = new Map<string, OrchestratorSession>();
  private readonly controllers = new Map<string, AbortController>();

  // Fine-grained: one Set<listener> per run id. A change to run A notifies
  // only A's bucket, not B's.
  private readonly runListeners = new Map<string, Set<() => void>>();

  // ── Global runs-list state ───────────────────────────────────────────────

  // Maintained independently of per-run sessions. Loading the list does not
  // pollute any individual run's reducer state.
  private runsList: OrchestratorRun[] = [];
  private runsListLoading = false;
  private readonly runsListListeners = new Set<() => void>();

  // ── Subscription ─────────────────────────────────────────────────────────

  /**
   * Subscribe to state changes for a single run.
   * Returns the unsubscribe function expected by useSyncExternalStore.
   */
  subscribeToRun(runId: string, listener: () => void): () => void {
    let bucket = this.runListeners.get(runId);
    if (bucket === undefined) {
      bucket = new Set();
      this.runListeners.set(runId, bucket);
    }
    bucket.add(listener);
    return () => {
      this.runListeners.get(runId)?.delete(listener);
    };
  }

  /**
   * Subscribe to changes in the global runs list.
   * Returns the unsubscribe function expected by useSyncExternalStore.
   */
  subscribeToRunsList(listener: () => void): () => void {
    this.runsListListeners.add(listener);
    return () => {
      this.runsListListeners.delete(listener);
    };
  }

  private notifyRun(runId: string): void {
    this.runListeners.get(runId)?.forEach(fn => fn());
  }

  private notifyRunsList(): void {
    this.runsListListeners.forEach(fn => fn());
  }

  // ── Run lifecycle ─────────────────────────────────────────────────────────

  /**
   * Register a fresh idle session for `runId`.
   * No-op if the run is already registered — safe to call on every mount.
   */
  register(runId: string): void {
    if (!this.sessions.has(runId)) {
      this.sessions.set(runId, createPerRunEmptySession());
      this.notifyRun(runId);
    }
  }

  /**
   * Seed a run's session directly from pre-replayed event history.
   * Replaces any existing session for that runId.
   * Used by HistoricalOrchestratorThread after it replays persisted events.
   */
  seed(runId: string, session: OrchestratorSession): void {
    this.sessions.set(runId, session);
    this.notifyRun(runId);
  }

  /**
   * Swap a client-side draft id for the real server-assigned run id.
   *
   * After calling this:
   *  - Subscribers of `draftId` receive UNREGISTERED_SESSION (the draft is gone).
   *  - Subscribers of `realId` receive the promoted session.
   *  - The caller must update its local state from `draftId` to `realId`.
   *
   * No-op for either id if it was never registered.
   */
  promoteRun(draftId: string, realId: string): void {
    const session = this.sessions.get(draftId);
    const controller = this.controllers.get(draftId);

    if (session !== undefined) {
      this.sessions.set(realId, session);
      this.sessions.delete(draftId);
    }
    if (controller !== undefined) {
      this.controllers.set(realId, controller);
      this.controllers.delete(draftId);
    }

    // Notify both so each set of subscribers sees its new state synchronously.
    this.notifyRun(draftId);
    this.notifyRun(realId);
  }

  /**
   * Remove a session and abort any live SSE stream.
   * Safe to call on an id that is not registered.
   */
  unregister(runId: string): void {
    this.controllers.get(runId)?.abort();
    this.controllers.delete(runId);
    this.sessions.delete(runId);

    // Final notification: subscribers receive UNREGISTERED_SESSION and can
    // render an empty / unmounted state.
    this.notifyRun(runId);

    // Release the listener bucket to avoid memory leaks for long chat sessions.
    this.runListeners.delete(runId);
  }

  // ── Dispatch ──────────────────────────────────────────────────────────────

  /**
   * Apply `action` to the session for `runId` via the shared reducer.
   * Auto-registers an empty session if `runId` is not yet tracked —
   * this handles the race where an SSE event arrives before the component's
   * mount effect calls register().
   */
  dispatch(runId: string, action: OrchestratorAction): void {
    const current = this.sessions.get(runId) ?? createPerRunEmptySession();
    const next = orchestratorReducer(current, action);
    this.sessions.set(runId, next);
    this.notifyRun(runId);
  }

  // ── Snapshots ────────────────────────────────────────────────────────────

  /**
   * Return the current session for `runId`, or the stable UNREGISTERED_SESSION
   * sentinel for unknown ids. The sentinel ensures useSyncExternalStore's
   * reference-equality bail-out works for unregistered consumers.
   */
  getSession(runId: string): OrchestratorSession {
    return this.sessions.get(runId) ?? UNREGISTERED_SESSION;
  }

  // ── AbortController management ────────────────────────────────────────────

  /**
   * Attach a new AbortController for the SSE stream on `runId`.
   * Aborts and replaces any previously attached controller.
   * Notifies run subscribers so isStreaming updates on the next render.
   */
  setAbortController(runId: string, controller: AbortController): void {
    this.controllers.get(runId)?.abort();
    this.controllers.set(runId, controller);
    this.notifyRun(runId);
  }

  /**
   * Abort the stream for `runId` without unregistering the session.
   * Use this for explicit user cancellation; use unregister() for cleanup.
   */
  abortRun(runId: string): void {
    this.controllers.get(runId)?.abort();
    this.controllers.delete(runId);
    this.notifyRun(runId);
  }

  /**
   * True if a non-aborted AbortController is currently attached to `runId`.
   * Computed synchronously — not a subscription — so reads the live value
   * at render time after any notifyRun() has triggered a re-render.
   */
  isStreaming(runId: string): boolean {
    const ctrl = this.controllers.get(runId);
    return ctrl !== undefined && !ctrl.signal.aborted;
  }

  // ── Global runs list ──────────────────────────────────────────────────────

  getRunsList(): OrchestratorRun[] {
    return this.runsList;
  }

  getRunsListLoading(): boolean {
    return this.runsListLoading;
  }

  setRunsList(runs: OrchestratorRun[], loading = false): void {
    this.runsList = runs;
    this.runsListLoading = loading;
    this.notifyRunsList();
  }

  setRunsListLoading(loading: boolean): void {
    this.runsListLoading = loading;
    this.notifyRunsList();
  }

  // ── Utility ───────────────────────────────────────────────────────────────

  hasRun(runId: string): boolean {
    return this.sessions.has(runId);
  }
}

// ─── React context ────────────────────────────────────────────────────────────

const RegistryContext = createContext<OrchestratorRegistry | null>(null);

function useRegistry(): OrchestratorRegistry {
  const ctx = useContext(RegistryContext);
  if (ctx === null) {
    throw new Error(
      'OrchestratorRegistry hooks must be used within <OrchestratorRegistryProvider>.',
    );
  }
  return ctx;
}

// ─── Provider ─────────────────────────────────────────────────────────────────

/**
 * Mount once near the app root — alongside the existing OrchestratorProvider
 * during the Phase 1 transition period.
 *
 * The registry instance is created lazily in a ref on first render and never
 * recreated, which means:
 *  - No subscriber churn on parent re-renders.
 *  - All sessions, controllers, and listeners survive for the app's lifetime.
 */
export function OrchestratorRegistryProvider({ children }: { children: ReactNode }) {
  const registryRef = useRef<OrchestratorRegistry | null>(null);
  if (registryRef.current === null) {
    registryRef.current = new OrchestratorRegistry();
  }

  return (
    <RegistryContext.Provider value={registryRef.current}>
      {children}
    </RegistryContext.Provider>
  );
}

// ─── useOrchestratorRun ───────────────────────────────────────────────────────

export interface UseOrchestratorRunReturn {
  /** Current reducer state for this run. */
  session: OrchestratorSession;
  /**
   * Dispatch an action to this run's session only.
   * Reference-stable unless `runId` changes.
   */
  dispatch: Dispatch<OrchestratorAction>;
  /** Abort the live SSE stream for this run without unregistering the session. */
  abort: () => void;
  /**
   * Attach a new AbortController for the SSE stream.
   * Aborts and replaces any existing controller for this run.
   */
  setAbortController: (controller: AbortController) => void;
  /**
   * Whether a live, non-aborted SSE stream is currently attached.
   * Derived synchronously from the registry; becomes accurate on the next
   * render after setAbortController() or abort() triggers a re-render.
   */
  isStreaming: boolean;
}

/**
 * Subscribe to a single orchestrator run's state.
 *
 * Granular isolation: dispatching to run A does not cause subscribers of
 * run B to re-render. Each hook instance holds its own subscription to one
 * run's notification bucket.
 *
 * If `runId` is not yet registered (e.g. during the brief window between
 * render and the mount effect calling register()), the hook returns the idle
 * sentinel session without throwing — components can render a loading state.
 */
export function useOrchestratorRun(runId: string): UseOrchestratorRunReturn {
  const registry = useRegistry();

  // Stable callbacks keyed on [registry, runId]. registry is ref-held and
  // never changes; runId changes are intentional and produce new subscriptions.
  const subscribe = useCallback(
    (onChange: () => void) => registry.subscribeToRun(runId, onChange),
    [registry, runId],
  );
  const getSnapshot = useCallback(
    () => registry.getSession(runId),
    [registry, runId],
  );

  // Third argument (getServerSnapshot) mirrors getSnapshot — in Tauri the
  // renderer hydrates from the same data source as the client.
  const session = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const dispatch: Dispatch<OrchestratorAction> = useCallback(
    (action) => registry.dispatch(runId, action),
    [registry, runId],
  );

  const abort = useCallback(
    () => registry.abortRun(runId),
    [registry, runId],
  );

  const setAbortController = useCallback(
    (controller: AbortController) => registry.setAbortController(runId, controller),
    [registry, runId],
  );

  // isStreaming is read synchronously because setAbortController / abortRun
  // already call notifyRun, which triggers a re-render — so this value is
  // always fresh by the time the component reads it.
  const isStreaming = registry.isStreaming(runId);

  return { session, dispatch, abort, setAbortController, isStreaming };
}

// ─── useOrchestratorRegistry ──────────────────────────────────────────────────

export interface UseOrchestratorRegistryReturn {
  /** Register a fresh idle session. No-op if already registered. */
  register: (runId: string) => void;
  /** Seed a run from a pre-built historical session. */
  seed: (runId: string, session: OrchestratorSession) => void;
  /**
   * Promote a draft id to the real server-assigned run id.
   * The caller must switch its local state to `realId` after this returns.
   */
  promoteRun: (draftId: string, realId: string) => void;
  /** Unregister a run and abort its stream. */
  unregister: (runId: string) => void;
  /** True if `runId` is currently registered. */
  hasRun: (runId: string) => boolean;
}

/**
 * Imperative registry lifecycle operations.
 *
 * Typical usage:
 *   - OrchestratorThread: register on mount, unregister on unmount.
 *   - Streaming hook: promoteRun when the server returns the real id.
 *   - HistoricalOrchestratorThread: seed after replaying persisted events.
 */
export function useOrchestratorRegistry(): UseOrchestratorRegistryReturn {
  const registry = useRegistry();

  const register = useCallback(
    (runId: string) => registry.register(runId),
    [registry],
  );
  const seed = useCallback(
    (runId: string, session: OrchestratorSession) => registry.seed(runId, session),
    [registry],
  );
  const promoteRun = useCallback(
    (draftId: string, realId: string) => registry.promoteRun(draftId, realId),
    [registry],
  );
  const unregister = useCallback(
    (runId: string) => registry.unregister(runId),
    [registry],
  );
  const hasRun = useCallback(
    (runId: string) => registry.hasRun(runId),
    [registry],
  );

  return { register, seed, promoteRun, unregister, hasRun };
}

// ─── useOrchestratorRunsList ──────────────────────────────────────────────────

export interface UseOrchestratorRunsListReturn {
  runs: OrchestratorRun[];
  loading: boolean;
  setRuns: (runs: OrchestratorRun[], loading?: boolean) => void;
  setLoading: (loading: boolean) => void;
}

/**
 * Subscribe to the global orchestrator runs list.
 *
 * This state lives at the registry level and is entirely independent of any
 * individual run's session — fetching the runs list does not dispatch into any
 * per-run reducer and does not cause any OrchestratorThread to re-render.
 *
 * Intended for the RunsList panel and chat history sidebar.
 */
export function useOrchestratorRunsList(): UseOrchestratorRunsListReturn {
  const registry = useRegistry();

  const subscribe = useCallback(
    (onChange: () => void) => registry.subscribeToRunsList(onChange),
    [registry],
  );

  // Returning the same array/boolean reference when nothing has changed is
  // safe because setRunsList always assigns a new array (never mutates) and
  // setRunsListLoading assigns a new boolean value.
  const getRuns = useCallback(() => registry.getRunsList(), [registry]);
  const getLoading = useCallback(() => registry.getRunsListLoading(), [registry]);

  const runs = useSyncExternalStore(subscribe, getRuns, getRuns);
  const loading = useSyncExternalStore(subscribe, getLoading, getLoading);

  const setRuns = useCallback(
    (r: OrchestratorRun[], l?: boolean) => registry.setRunsList(r, l),
    [registry],
  );
  const setLoading = useCallback(
    (l: boolean) => registry.setRunsListLoading(l),
    [registry],
  );

  return { runs, loading, setRuns, setLoading };
}
