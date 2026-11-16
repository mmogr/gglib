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
 *
 * An event carries the port and nothing else, so each one moves the
 * registry at once and then asks the daemon for the rest (the state word,
 * the reached address, the key rule, the pin), as the remote bridge does.
 */

import { bridgeEvents } from './bridgeEvents';
import { getTransport } from './transport';
import { applyProxyStatus, ingestProxyEvent, resetProxyState } from './proxyRegistry';

const bridge = bridgeEvents({
  category: 'proxy',
  onEvent: (evt) => {
    ingestProxyEvent(evt);
    // The event overtook any read still out, so that answer is dropped in
    // favour of this one's.
    void bridge.refresh();
  },
  // Hydration — seed initial state from the daemon's whole answer
  read: () => getTransport().getProxyStatus(),
  apply: applyProxyStatus,
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
