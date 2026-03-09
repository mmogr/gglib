/**
 * Proxy Events Initialization
 *
 * Bridges SSE proxy events into the proxyRegistry store.
 * Proxy always uses HTTP/axum (no Tauri commands), so events are
 * SSE-only on both web and desktop — no platform branching needed.
 *
 * Uses subscribeSseEvent directly (not subscribeToEvent) because in Tauri
 * the platform-selected transport routes to Tauri IPC, which never receives
 * proxy events. The Rust backend emits proxy events exclusively via
 * SseBroadcaster, not TauriEventEmitter.
 *
 * Hydration race fix: subscribe FIRST, then fetch initial status.
 * An eventVersion guard drops stale hydration data if a real event
 * arrived before the fetch response.
 */

import { subscribeSseEvent } from './transport/events/sse';
import { getProxyStatus } from './clients/servers';
import { ingestProxyEvent, resetProxyState } from './proxyRegistry';
import type { Unsubscribe } from './transport/types/common';
import type { ProxyEvent } from './transport/types/events';

let unsubscribe: Unsubscribe | null = null;
let eventVersion = 0;

/**
 * Initialize proxy event handling.
 * Safe to call multiple times — only initializes once.
 */
export function initProxyEvents(): void {
  if (unsubscribe) return;

  eventVersion = 0;

  // 1. Subscribe FIRST so no events are missed during hydration fetch
  unsubscribe = subscribeSseEvent('proxy', (evt: ProxyEvent) => {
    eventVersion++;
    ingestProxyEvent(evt);
  });

  // 2. Hydration fetch — seed initial state from current backend status
  const versionBeforeFetch = eventVersion;
  getProxyStatus()
    .then((status) => {
      // Drop stale hydration if a real event already arrived
      if (eventVersion !== versionBeforeFetch) return;

      if (status.running) {
        ingestProxyEvent({ type: 'proxy_started', port: status.port });
      }
    })
    .catch(() => {
      // Hydration failure is non-fatal — events will correct state
    });
}

/**
 * Cleanup proxy event handling.
 * Should be called on app unmount or hot-reload.
 */
export function cleanupProxyEvents(): void {
  if (unsubscribe) {
    unsubscribe();
    unsubscribe = null;
  }
  eventVersion = 0;
  resetProxyState();
}
