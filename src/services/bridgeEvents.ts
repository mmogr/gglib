/**
 * How a store that reads the daemon is kept current (`keptCurrent`), and the
 * bridge from one category of the daemon's events into a registry
 * (`bridgeEvents`), which is kept current the same way.
 *
 * A store reads now, and again each time the event stream opens: the first
 * connection and every reconnection. The stream opening is the one signal
 * that the daemon answers, so a first read it refused (the window ran
 * before the service listened, on a cold start) is followed by one that
 * lands, and a daemon that came back, perhaps as a new process, is read
 * afresh.
 *
 * While the stream is open the daemon answers, so a read that fails then (a
 * request that timed out, a 5xx) is asked again here, after a wait that
 * grows from 1 s, doubling, to 30 s (plus up to 250 ms of jitter), until a
 * read lands, an event or a write overtakes it, the stream closes (its next
 * opening reads anyway) or the store stops keeping. One retry is pending per
 * store at most, and none while the stream is down. This is the one timer a
 * store's reads have: no store has one of its own.
 *
 * The event stream has no backlog, so a registry starts from a GET; and a GET
 * says what was true when it was answered, which an event that arrived since
 * has already replaced. So the order is fixed, and written here once:
 * subscribe first, then read, and apply the read only if nothing happened
 * while it was out.
 *
 * "Nothing" is counted. Every event moves `version`, and so do a write and a
 * stop (`overtake`), and nothing moves it back; a read is applied only if
 * `version` is where the read began, and only if no read begun after it has
 * landed. So an older answer never lands on a newer event or a newer read,
 * and never in a store that has stopped since.
 */

import { getTransport } from './transport';
import { Backoff } from './transport/events/backoff';
import type { Unsubscribe } from './transport/types/common';
import type { AppEventMap, AppEventType } from './transport/types/events';

/**
 * What became of one read.
 *
 * `superseded` is not a failure: the read worked, and an event, a write, a
 * stop or a later read overtook it, so its answer was dropped.
 */
export type Reread = 'applied' | 'superseded' | 'failed';

interface KeptCurrentOptions<T> {
  /** The GET that says what is true now. */
  read: () => Promise<T>;
  /** Writes a read's answer into the store. Never called for a read that was overtaken. */
  apply: (value: T) => void;
  /**
   * Told why a read failed, for a store that says so. Not told when a read
   * begun after it has already landed, since the store holds newer state.
   * A failure is never newer state: an earlier read that lands after it is
   * still applied.
   */
  onFailure?: (error: unknown) => void;
  /** Runs each time the stream opens, before the read the opening sets going. */
  onStreamOpen?: () => void;
}

/** The first wait before a failed read is asked again while the stream is open, and the longest. */
export const RETRY_MIN_MS = 1000;
export const RETRY_MAX_MS = 30_000;

export interface KeptCurrent {
  /**
   * Read now, and again each time the stream opens, and ask a failed read
   * again while it is open, until the returned stop.
   */
  keep: () => Unsubscribe;
  /** Read again. Never rejects, so an ignored result cannot become an unhandled one. */
  refresh: () => Promise<Reread>;
  /**
   * Disown whatever read is out, and drop a pending retry: something newer
   * than it has happened (an event, a write).
   */
  overtake: () => void;
}

/** While a store is kept: whether the stream is open, and the one retry that may be pending. */
interface Keeping {
  open: boolean;
  retry: ReturnType<typeof setTimeout> | null;
}

export function keptCurrent<T>({ read, apply, onFailure, onStreamOpen }: KeptCurrentOptions<T>): KeptCurrent {
  let version = 0;
  /** Reads begun, and the number of the newest one applied. */
  let begun = 0;
  let landed = 0;
  let keeping: Keeping | null = null;
  const backoff = new Backoff(RETRY_MIN_MS, RETRY_MAX_MS);

  function cancelRetry(): void {
    if (keeping?.retry != null) {
      clearTimeout(keeping.retry);
      keeping.retry = null;
    }
  }

  /** Ask again later, while the stream is open and no retry is pending. */
  function retryLater(): void {
    const k = keeping;
    if (!k || !k.open || k.retry !== null) return;
    k.retry = setTimeout(() => {
      k.retry = null;
      if (keeping === k && k.open) void refresh();
    }, backoff.next());
  }

  async function refresh(): Promise<Reread> {
    const startedAt = version;
    const mine = ++begun;
    let value: T;
    try {
      value = await read();
    } catch (error) {
      // Newer state, or a newer read still out, says more than this failure.
      if (version === startedAt && mine > landed) {
        onFailure?.(error);
        if (mine === begun) retryLater();
      }
      return 'failed';
    }
    if (version !== startedAt || mine < landed) return 'superseded';
    landed = mine;
    backoff.reset();
    cancelRetry();
    try {
      apply(value);
      return 'applied';
    } catch {
      return 'failed';
    }
  }

  function overtake(): void {
    version++;
    cancelRetry();
  }

  function keep(): Unsubscribe {
    const transport = getTransport();
    const k: Keeping = { open: transport.isEventStreamOpen(), retry: null };
    keeping = k;
    const stopOpen = transport.onEventStreamOpen(() => {
      if (keeping !== k) return;
      k.open = true;
      cancelRetry();
      backoff.reset();
      onStreamOpen?.();
      void refresh();
    });
    const stopClose = transport.onEventStreamClose(() => {
      if (keeping !== k) return;
      // Its next opening reads anyway.
      k.open = false;
      cancelRetry();
    });
    void refresh();
    return () => {
      stopOpen();
      stopClose();
      overtake();
      if (keeping === k) keeping = null;
    };
  }

  return { keep, refresh, overtake };
}

interface EventBridgeOptions<K extends AppEventType, T> extends KeptCurrentOptions<T> {
  /** The event category to listen to. */
  category: K;
  /** Takes one event of that category into the registry. */
  onEvent: (evt: AppEventMap[K]) => void;
  /** Empties the registry on cleanup, for one that should not outlive its bridge. */
  reset?: () => void;
  /**
   * Whether an event makes a read that is out stale. Every event does,
   * unless this says otherwise: a registry whose reads carry their own order
   * (a revision) need not drop one for an event that says nothing of it.
   */
  overtakes?: (evt: AppEventMap[K]) => boolean;
}

interface EventBridge {
  /** Subscribe, then read, and read again each time the stream opens. Only the first call does anything, until `cleanup`. */
  init: () => void;
  /** Unsubscribe, stop reading on openings, and disown whatever read is still out. */
  cleanup: () => void;
  /** Read again. Never rejects, so an ignored result cannot become an unhandled one. */
  refresh: () => Promise<Reread>;
}

export function bridgeEvents<K extends AppEventType, T>({
  category,
  onEvent,
  read,
  apply,
  onFailure,
  onStreamOpen,
  reset,
  overtakes,
}: EventBridgeOptions<K, T>): EventBridge {
  const kept = keptCurrent({ read, apply, onFailure, onStreamOpen });
  let unsubscribe: Unsubscribe | null = null;
  let stopKeeping: Unsubscribe | null = null;

  function init(): void {
    if (unsubscribe) return;

    // Subscribe FIRST so no event is missed during the hydration read below.
    unsubscribe = getTransport().subscribe(category, (evt) => {
      if (overtakes?.(evt) ?? true) kept.overtake();
      onEvent(evt);
    });
    stopKeeping = kept.keep();
  }

  function cleanup(): void {
    if (unsubscribe) {
      unsubscribe();
      unsubscribe = null;
    }
    stopKeeping?.();
    stopKeeping = null;
    kept.overtake();
    reset?.();
  }

  return { init, cleanup, refresh: kept.refresh };
}
