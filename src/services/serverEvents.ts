/**
 * Server lifecycle events, from the daemon into the registry.
 *
 * One path for every mode. Server lifecycle is the daemon's news, and the
 * daemon tells every client the same way — `/api/events`, over SSE, through
 * `services/transport`. The desktop app is a client like any other.
 *
 * It used to branch: `isDesktop()` took a Tauri-event adapter that listened
 * for `server:snapshot|started|stopped|error|health_changed` on the Tauri bus.
 * Nothing emitted those. The GUI's own backend had been consolidated into the
 * daemon, and with it went the `AppEventEmitter` implementation that would
 * have. So the desktop branch registered listeners for events that could never
 * fire and skipped the subscription that works — leaving `useServerState`,
 * `useIsServerRunning` and the health indicator inert in the app, and only in
 * the app. The web build was fine, which is why it went unnoticed.
 *
 * Nothing here is desktop-aware any more, and that is the point: a second path
 * is a second thing to keep true.
 */

import { bridgeEvents } from './bridgeEvents';
import { createEventStore } from './createEventStore';
import { getTransport } from './transport';
import { formatError } from '../utils/errors';
import { ingestServerEvent } from './serverRegistry';
import {
  normalizeServerEventFromAppEvent,
  normalizeServerSnapshotFromList,
} from './serverEvents.normalize';

/**
 * Whether the list has been read since the registry was last told to forget
 * it: the difference between no servers running and not knowing yet.
 */
const listHeard = createEventStore(false);
/** Why the last read of the list failed, while none has landed since; `null` otherwise. */
const listError = createEventStore<string | null>(null);

// The ordering is `bridgeEvents`'s: subscribe first, then fetch the list, and
// drop a list that a live event or a cleanup overtook; read it again each
// time the stream opens. No `reset`: this registry keeps what it holds
// across a cleanup.
//
// Until a list has been read, the registry cannot tell "nothing running"
// from "not known", and a page waits on it (a chat's address). So a read
// that failed or that an event overtook must be followed by another, and it
// is, with no timer of this module's own: `keptCurrent` asks a failed read
// again while the stream is open, the next opening reads again, and so does
// every server event while the list is unheard. An event says the daemon
// answers, and it overtook the read that was out, so the read it sets going
// is the one that can land; there is at most one per event.
const bridge = bridgeEvents({
  category: 'server',
  onEvent: (payload) => {
    const normalized = normalizeServerEventFromAppEvent(payload);
    if (normalized) {
      ingestServerEvent(normalized);
    }
    if (!listHeard.getState()) void bridge.refresh();
  },
  // Hydration: seed the registry with servers already running at load. No
  // event carries those, so they are read from the REST list, which has the
  // normalizer's own entry point for one: it is snake_case where the event
  // frames are camelCase.
  read: () => getTransport().listServers(),
  apply: (servers) => {
    ingestServerEvent(normalizeServerSnapshotFromList(servers));
    listHeard.setState(true);
    listError.setState(null);
  },
  onFailure: (err) => listError.setState(formatError(err)),
});

/** Whether a read of the list has landed: until one has, an empty registry is not "nothing running". */
export function useServerListHeard(): boolean {
  return listHeard.useStore();
}

/** Why the last read of the list failed, until one lands; `null` when none has failed since. */
export function useServerListError(): string | null {
  return listError.useStore();
}

/** Forget that the list was read, for a reader whose daemon went away; the next read sets it again. */
export function forgetServerList(): void {
  listHeard.setState(false);
  listError.setState(null);
}

/** Read the list again now, as hydration does: a reader's Retry. */
export function refreshServers(): void {
  void bridge.refresh();
}

/**
 * Start bridging server lifecycle events into the registry.
 *
 * Safe to call multiple times — only the first call does anything.
 */
export function initServerEvents(): void {
  bridge.init();
}

/**
 * Stop bridging server lifecycle events. Call on app unmount.
 */
export function cleanupServerEvents(): void {
  bridge.cleanup();
}

// Re-export registry types and hooks for convenience
export {
  type ServerEvent,
  type ServerState,
  type ServerStatus,
  type ServerStateInfo,
  useServerState,
  useIsServerRunning,
  isServerRunning,
  getServerState,
} from './serverRegistry';
