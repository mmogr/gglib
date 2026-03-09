/**
 * Server Events Initialization
 * TRANSPORT_EXCEPTION: Auto-selects the appropriate event adapter (Tauri only) based on platform.
 * Uses platform detection to initialize server lifecycle event handling.
 * 
 * Note: SSE adapter has been removed. Server events are now handled through
 * the unified transport layer in services/transport/events/sse.ts
 */

import { isDesktop } from './platform';
import { initTauriServerEvents, cleanupTauriServerEvents } from './serverEvents.tauri';
import { subscribeToEvent } from './clients/events';
import { listServers } from './clients/servers';
import type { Unsubscribe } from './transport/types/common';
import { ingestServerEvent } from './serverRegistry';
import { normalizeServerEventFromAppEvent } from './serverEvents.normalize';

let initialized = false;
let webUnsubscribe: Unsubscribe | null = null;
let webEventVersion = 0;

/**
 * Initialize server lifecycle event handling.
 * Desktop app uses Tauri events; web uses unified SSE transport.
 * Safe to call multiple times - will only initialize once.
 */
export async function initServerEvents(): Promise<void> {
  if (initialized) {
    return;
  }

  if (isDesktop()) {
    await initTauriServerEvents();
  }
  // Web: Bridge canonical backend AppEvent server lifecycle events into serverRegistry.
  // This enables UI (e.g. Chat composer) to reactively switch to read-only when servers stop.
  else {
    webEventVersion = 0;

    // 1. Subscribe FIRST so no events are missed during hydration fetch
    webUnsubscribe = subscribeToEvent('server', (payload) => {
      webEventVersion++;
      const normalized = normalizeServerEventFromAppEvent(payload as unknown);
      if (normalized) {
        ingestServerEvent(normalized);
      }
    });

    // 2. Hydration fetch — seed registry with servers already running on page load
    const versionBeforeFetch = webEventVersion;
    listServers()
      .then((servers) => {
        // Drop stale hydration if a live server event already arrived
        if (webEventVersion !== versionBeforeFetch) return;
        ingestServerEvent({
          type: 'snapshot',
          servers: servers.map((s) => ({
            modelId: String(s.modelId),
            status: 'running' as const,
            port: s.port,
            updatedAt: Date.now(),
            modelName: s.modelName,
          })),
        });
      })
      .catch(() => {
        // Non-fatal — live events will populate state as servers start
      });
  }

  initialized = true;
}

/**
 * Cleanup server event handling.
 * Should be called on app unmount.
 */
export function cleanupServerEvents(): void {
  if (isDesktop()) {
    cleanupTauriServerEvents();
  }
  if (webUnsubscribe) {
    webUnsubscribe();
    webUnsubscribe = null;
  }
  webEventVersion = 0;
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
  getAllServerStates,
} from './serverRegistry';
