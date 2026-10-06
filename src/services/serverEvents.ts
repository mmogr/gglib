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
import { getTransport } from './transport';
import { ingestServerEvent } from './serverRegistry';
import {
  normalizeServerEventFromAppEvent,
  normalizeServerSnapshotFromList,
} from './serverEvents.normalize';

// The ordering is `bridgeEvents`'s: subscribe first, then fetch the list, and
// drop a list that a live event or a cleanup overtook. No `reset`: this
// registry keeps what it holds across a cleanup.
const bridge = bridgeEvents({
  category: 'server',
  onEvent: (payload) => {
    const normalized = normalizeServerEventFromAppEvent(payload);
    if (normalized) {
      ingestServerEvent(normalized);
    }
  },
  // Hydration: seed the registry with servers already running at load. No
  // event carries those, so they are read from the REST list, which has the
  // normalizer's own entry point for one: it is snake_case where the event
  // frames are camelCase.
  read: () => getTransport().listServers(),
  apply: (servers) => ingestServerEvent(normalizeServerSnapshotFromList(servers)),
});

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
