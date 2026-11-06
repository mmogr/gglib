/**
 * Proxy State Registry
 *
 * Event-driven store for the endpoint (the proxy). Uses createEventStore
 * to share the same subscribe/use pattern as serverRegistry.
 *
 * Two kinds of fact live here. `running` and `port` move at once on a proxy
 * event (ProxyStarted, ProxyStopped, ProxyCrashed); they are what every
 * surface read before the status said more. `status` is the daemon's whole
 * answer (`ProxyStatus`: the state word, where a client reaches it, whether
 * a request needs the key, the pin), read on hydration and again after each
 * event, since an event carries only the port. An event drops the status it
 * finds: it belonged to the run before the event, and a stop followed at
 * once by a start would otherwise show the old run's address and key rule
 * until a read landed. So between an event and the read that follows it the
 * registry knows only what the event said, and `endpointFacts` says only
 * that.
 *
 * @module services/proxyRegistry
 */

import { createEventStore } from './createEventStore';
import type { ProxyEvent } from './transport/types/events';
import type { ProxyStatus } from './transport/types/proxy';
import type { EndpointState } from '../types/generated/EndpointState';
import type { KeyState } from '../types/generated/KeyState';

export interface ProxyState {
  running: boolean;
  port: number | null;
  /** The daemon's last full answer, or `null` before the first read. */
  status: ProxyStatus | null;
}

const INITIAL: ProxyState = { running: false, port: null, status: null };

const store = createEventStore<ProxyState>(INITIAL);

/**
 * Ingest a proxy event and update state. The status held is dropped: it
 * described the run before this event, and the read the bridge makes after
 * the event replaces it.
 */
export function ingestProxyEvent(evt: ProxyEvent): void {
  switch (evt.type) {
    case 'proxy_started':
      store.setState({ running: true, port: evt.port, status: null });
      break;
    case 'proxy_stopped':
    case 'proxy_crashed':
      store.setState({ running: false, port: null, status: null });
      break;
  }
}

/**
 * Take the daemon's status as the whole truth: the read that hydrates the
 * registry and the one that follows each event.
 */
export function applyProxyStatus(status: ProxyStatus): void {
  store.setState({ running: status.running, port: status.running ? status.port : null, status });
}

/** Reset proxy state (used during cleanup / hot-reload). */
export function resetProxyState(): void {
  store.setState(INITIAL);
}

/** React hook — subscribe to the full proxy state. */
export function useProxyState(): ProxyState {
  return store.useStore();
}

/** The proxy state without React, for a reader outside a component. */
export function getProxyState(): ProxyState {
  return store.getState();
}

/**
 * What the endpoint line says, read from the two kinds of fact together.
 *
 * - `state`: the daemon's word once its status has been read for this run;
 *   between an event and the read after it, `on` or `off` from the event
 *   alone, since the status will say whether the run is pinned.
 * - `baseUrl`: where a client reaches it, only while up and only from the
 *   daemon (the dial rule is the daemon's: a wildcard bind is reached on
 *   loopback). `null` while off, and while up until the status has said.
 * - `key`: whether a request must carry the key, the same way.
 * - `pinnedModel`: the model a pinned run serves, else `null`.
 *
 * A status that disagrees with `running` cannot be held (an event drops the
 * status, a read sets both), but the guard stays so a reader never trusts
 * one that somehow is.
 */
export interface EndpointFacts {
  state: EndpointState;
  baseUrl: string | null;
  key: KeyState | null;
  pinnedModel: string | null;
}

export function endpointFacts({ running, status }: ProxyState): EndpointFacts {
  if (!running) return { state: 'off', baseUrl: null, key: null, pinnedModel: null };
  if (status === null || !status.running) {
    return { state: 'on', baseUrl: null, key: null, pinnedModel: null };
  }
  // A daemon older than this GUI answers without the fields E2 added. The
  // two ship together, but a dev build adopting an installed daemon does
  // not, and a missing field is then "not said", never a crash.
  return {
    state: status.state ?? (status.pinned_model ? 'pinned' : 'on'),
    baseUrl: status.base_url ?? null,
    key: status.key ?? null,
    pinnedModel: status.pinned_model ?? null,
  };
}

/** React hook — the endpoint line's facts, derived on every change. */
export function useEndpointFacts(): EndpointFacts {
  return endpointFacts(store.useStore());
}
