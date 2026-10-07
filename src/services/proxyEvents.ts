/**
 * Proxy Events Initialization
 *
 * Bridges SSE proxy events into the proxyRegistry store.
 * Proxy always uses HTTP/axum (no Tauri commands), so events are
 * SSE-only on both web and desktop — no platform branching needed.
 * The Rust backend emits proxy events exclusively via SseBroadcaster.
 *
 * The ordering is `bridgeEvents`'s: subscribe FIRST, then fetch the status,
 * and drop a status that a real event or a cleanup overtook.
 */

import { bridgeEvents } from './bridgeEvents';
import { getTransport } from './transport';
import { ingestProxyEvent, resetProxyState } from './proxyRegistry';

const bridge = bridgeEvents({
  category: 'proxy',
  onEvent: ingestProxyEvent,
  // Hydration — seed initial state from current backend status
  read: () => getTransport().getProxyStatus(),
  apply: (status) => {
    // A running proxy always reports a port — `to_api_status` sets the two
    // together — but they are separate fields, so narrow on the one being
    // read rather than trusting the pair. The impossible case skips
    // hydration, which live events would correct anyway.
    if (status.running && status.port !== null) {
      ingestProxyEvent({ type: 'proxy_started', port: status.port });
    }
  },
  reset: resetProxyState,
});

/**
 * Initialize proxy event handling.
 * Safe to call multiple times — only initializes once.
 */
export function initProxyEvents(): void {
  bridge.init();
}

/**
 * Cleanup proxy event handling.
 * Should be called on app unmount or hot-reload.
 */
export function cleanupProxyEvents(): void {
  bridge.cleanup();
}
