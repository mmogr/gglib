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

let initialized = false;

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
  // Web: Server events now handled by unified transport layer

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
  // Web: No cleanup needed - handled by transport layer
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
