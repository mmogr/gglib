/**
 * Server Events Initialization
 *
 * Auto-selects the appropriate event adapter (Tauri or SSE) based on platform
 * and initializes server lifecycle event handling.
 */

import { isTauriApp } from '../utils/platform';
import { initTauriServerEvents, cleanupTauriServerEvents } from './serverEvents.tauri';
import { initSseServerEvents, cleanupSseServerEvents } from './serverEvents.sse';

let initialized = false;

/**
 * Initialize server lifecycle event handling.
 * Automatically selects Tauri events for desktop app, SSE for web.
 * Safe to call multiple times - will only initialize once.
 */
export async function initServerEvents(): Promise<void> {
  if (initialized) {
    return;
  }

  if (isTauriApp) {
    await initTauriServerEvents();
  } else {
    initSseServerEvents();
  }

  initialized = true;
}

/**
 * Cleanup server event handling.
 * Should be called on app unmount.
 */
export function cleanupServerEvents(): void {
  if (isTauriApp) {
    cleanupTauriServerEvents();
  } else {
    cleanupSseServerEvents();
  }
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
