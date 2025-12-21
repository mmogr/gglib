/**
 * EventBus factory.
 * Provides unified event subscription interface that works in both Tauri and web.
 * 
 * Future: migrate to fetch-based SSE with bearer auth for unified implementation.
 */

import { createSseEvents } from './sse';
import { createTauriEvents } from './tauri';
import type { EventsTransport } from '../types/events';

/**
 * Detect if running in Tauri environment.
 */
function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Create EventBus for the current environment.
 * Returns object matching EventsTransport interface.
 * 
 * Implementation:
 * - Tauri: uses native listen() for IPC events
 * - Web: uses SSE (Server-Sent Events) from HTTP endpoint
 */
export function createEventBus(): EventsTransport {
  if (isTauri()) {
    return createTauriEvents();
  } else {
    return createSseEvents();
  }
}

// Legacy exports for backward compatibility during migration
export { subscribeTauriEvent } from './tauri';
export { subscribeSseEvent } from './sse';

