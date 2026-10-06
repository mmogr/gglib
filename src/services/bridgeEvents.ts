/**
 * The bridge from one category of the daemon's events into a registry.
 *
 * The event stream has no backlog, so a registry starts from a GET; and a GET
 * says what was true when it was answered, which an event that arrived since
 * has already replaced. So the order is fixed, and written here once:
 * subscribe first, then read, and apply the read only if nothing happened
 * while it was out.
 *
 * "Nothing" is counted. Every event moves `version`, and so does `cleanup`,
 * and nothing moves it back; a read is applied only if `version` is where the
 * read began. So an older answer never lands on a newer event, and never in a
 * registry whose bridge has been taken down since.
 */

import { getTransport } from './transport';
import type { Unsubscribe } from './transport/types/common';
import type { AppEventMap, AppEventType } from './transport/types/events';

/**
 * What became of one read.
 *
 * `superseded` is not a failure: the read worked, and an event or a cleanup
 * overtook it, so its answer was dropped.
 */
type Reread = 'applied' | 'superseded' | 'failed';

interface EventBridgeOptions<K extends AppEventType, T> {
  /** The event category to listen to. */
  category: K;
  /** Takes one event of that category into the registry. */
  onEvent: (evt: AppEventMap[K]) => void;
  /** The GET that says what is true now. */
  read: () => Promise<T>;
  /** Writes a read's answer into the registry. Never called for a read that was overtaken. */
  apply: (value: T) => void;
  /** Empties the registry on cleanup, for one that should not outlive its bridge. */
  reset?: () => void;
}

interface EventBridge {
  /** Subscribe, then hydrate. Only the first call does anything, until `cleanup`. */
  init: () => void;
  /** Unsubscribe, and disown whatever read is still out. */
  cleanup: () => void;
  /** Read again. Never rejects, so an ignored result cannot become an unhandled one. */
  refresh: () => Promise<Reread>;
}

export function bridgeEvents<K extends AppEventType, T>({
  category,
  onEvent,
  read,
  apply,
  reset,
}: EventBridgeOptions<K, T>): EventBridge {
  let unsubscribe: Unsubscribe | null = null;
  let version = 0;

  async function refresh(): Promise<Reread> {
    const startedAt = version;
    try {
      const value = await read();
      if (version !== startedAt) return 'superseded';
      apply(value);
      return 'applied';
    } catch {
      // Non-fatal: the next event, or the next caller, reads again.
      return 'failed';
    }
  }

  function init(): void {
    if (unsubscribe) return;

    // Subscribe FIRST so no event is missed during the hydration read below.
    unsubscribe = getTransport().subscribe(category, (evt) => {
      version++;
      onEvent(evt);
    });
    void refresh();
  }

  function cleanup(): void {
    if (unsubscribe) {
      unsubscribe();
      unsubscribe = null;
    }
    version++;
    reset?.();
  }

  return { init, cleanup, refresh };
}
