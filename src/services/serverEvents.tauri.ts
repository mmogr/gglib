/**
 * Tauri Event Adapter for Server Lifecycle Events
 *
 * Listens to Tauri events and normalizes them to the shared ServerEvent type,
 * then ingests them into the server registry.
 *
 * Events listened to:
 * - server:snapshot - Snapshot of running servers
 * - server:started  - Server started and ready
 * - server:stopped  - Server stopped cleanly
 * - server:error    - Server encountered an error
 * - server:health_changed - Health status changed
 */

import { ingestServerEvent } from './serverRegistry';
import {
  normalizeServerEventFromNamedEvent,
  type CanonicalServerEventName,
} from './serverEvents.normalize';

type UnlistenFn = () => void;

let unlisteners: UnlistenFn[] = [];
let initialized = false;

/**
 * Initialize Tauri event listeners for server lifecycle events.
 * Safe to call multiple times - will only initialize once.
 */
export async function initTauriServerEvents(): Promise<void> {
  if (initialized) {
    return;
  }

  try {
    const { listen } = await import('@tauri-apps/api/event');

    const eventTypes: CanonicalServerEventName[] = [
      'server:snapshot',
      'server:started',
      'server:stopped',
      'server:error',
      'server:health_changed',
    ];

    for (const eventType of eventTypes) {
      const unlisten = await listen(eventType, (event) => {
        const normalized = normalizeServerEventFromNamedEvent(eventType, event.payload);
        if (normalized) {
          ingestServerEvent(normalized);
        }
      });
      unlisteners.push(unlisten);
    }

    initialized = true;
    console.debug('[serverEvents.tauri] Initialized server event listeners');
  } catch (error) {
    console.error('[serverEvents.tauri] Failed to initialize:', error);
  }
}

/**
 * Cleanup Tauri event listeners.
 * Called on app unmount or when switching to web mode.
 */
export function cleanupTauriServerEvents(): void {
  for (const unlisten of unlisteners) {
    unlisten();
  }
  unlisteners = [];
  initialized = false;
}
