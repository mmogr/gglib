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
 *
 * A start this window asks for (`startEndpoint`, `startEndpointPinned`)
 * takes the daemon's answer as the status at once, as a read would.
 */

import { bridgeEvents } from './bridgeEvents';
import { getTransport } from './transport';
import { applyProxyStatus, ingestProxyEvent, resetProxyState } from './proxyRegistry';
import type { ProxyConfig, StartPinnedRequest } from './transport/types/proxy';

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
 * Start the endpoint (as the settings say, for `{}`), and take the status the daemon
 * answers with as the registry's at once. The daemon says `proxy_started`
 * only when a start turns the endpoint on, not when it finds it on, and the
 * event can reach this window after the answer or not at all (a stream
 * still reconnecting), so the answer is what moves this window's line.
 */
export async function startEndpoint(config: Partial<ProxyConfig>): Promise<void> {
  applyStartAnswer(await getTransport().startProxy(config));
}

/** Start the endpoint pinned to one model, and take its answer the same way. */
export async function startEndpointPinned(request: StartPinnedRequest): Promise<void> {
  applyStartAnswer(await getTransport().startPinnedProxy(request));
}

/**
 * A start's answer is newer than any status read still out, which was asked
 * before the start: that read is dropped, so it cannot write "off" over it.
 */
function applyStartAnswer(status: Awaited<ReturnType<ReturnType<typeof getTransport>['startProxy']>>): void {
  bridge.overtake();
  applyProxyStatus(status);
}

/**
 * Cleanup proxy event handling.
 * Should be called on app unmount or hot-reload.
 */
export function cleanupProxyEvents(): void {
  bridge.cleanup();
}
