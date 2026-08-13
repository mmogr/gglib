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

import { getTransport } from './transport';
import type { Unsubscribe } from './transport/types/common';
import { ingestServerEvent } from './serverRegistry';
import { normalizeServerEventFromAppEvent } from './serverEvents.normalize';

let initialized = false;
let unsubscribe: Unsubscribe | null = null;
let eventVersion = 0;

/**
 * Start bridging server lifecycle events into the registry.
 *
 * Safe to call multiple times — only the first call does anything.
 */
export function initServerEvents(): void {
  if (initialized) {
    return;
  }

  eventVersion = 0;

  // Subscribe FIRST so no event is missed during the hydration fetch below.
  unsubscribe = getTransport().subscribe('server', (payload) => {
    eventVersion++;
    const normalized = normalizeServerEventFromAppEvent(payload as unknown);
    if (normalized) {
      ingestServerEvent(normalized);
    }
  });

  // Hydration: seed the registry with servers already running at load. Routed
  // through the tolerant normalizer so both camelCase and snake_case payloads
  // map correctly and malformed entries are dropped rather than becoming
  // "undefined" registry keys.
  const versionBeforeFetch = eventVersion;
  getTransport()
    .listServers()
    .then((servers) => {
      // Drop stale hydration if a live event already arrived.
      if (eventVersion !== versionBeforeFetch) return;
      const normalized = normalizeServerEventFromAppEvent({
        type: 'server_snapshot',
        servers,
      });
      if (normalized) {
        ingestServerEvent(normalized);
      }
    })
    .catch(() => {
      // Non-fatal — live events will populate state as servers start.
    });

  initialized = true;
}

/**
 * Stop bridging server lifecycle events. Call on app unmount.
 */
export function cleanupServerEvents(): void {
  if (unsubscribe) {
    unsubscribe();
    unsubscribe = null;
  }
  eventVersion = 0;
  initialized = false;
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
