/**
 * Proxy State Registry
 *
 * Event-driven store for proxy lifecycle state. Uses createEventStore
 * to share the same subscribe/use pattern as serverRegistry.
 *
 * State is updated exclusively by proxy events (ProxyStarted, ProxyStopped,
 * ProxyCrashed) — no polling, no manual state sync from action handlers.
 */

import { createEventStore } from './createEventStore';
import type { ProxyEvent } from './transport/types/events';

export interface ProxyState {
  running: boolean;
  port: number | null;
}

const INITIAL: ProxyState = { running: false, port: null };

const store = createEventStore<ProxyState>(INITIAL);

/** Ingest a proxy event and update state. */
export function ingestProxyEvent(evt: ProxyEvent): void {
  switch (evt.type) {
    case 'proxy_started':
      store.setState({ running: true, port: evt.port });
      break;
    case 'proxy_stopped':
    case 'proxy_crashed':
      store.setState({ running: false, port: null });
      break;
  }
}

/** Reset proxy state (used during cleanup / hot-reload). */
export function resetProxyState(): void {
  store.setState(INITIAL);
}

/** React hook — subscribe to the full proxy state. */
export function useProxyState(): ProxyState {
  return store.useStore();
}

/** Non-React accessor for the current proxy state. */
export function getProxyState(): ProxyState {
  return store.getState();
}

/** Subscribe to proxy state changes. Returns unsubscribe function. */
export const subscribeProxy = store.subscribe;
