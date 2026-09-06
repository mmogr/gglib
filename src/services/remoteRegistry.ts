/**
 * Remote Tunnel State Registry (ADR 0012)
 *
 * Event-driven store for the tunnel, both sides, on the same
 * `createEventStore` pattern as `proxyRegistry`. Two things are kept:
 *
 * - `status` — the daemon's `RemoteStatus`, whole. Events move it forward
 *   the moment they arrive; `remoteEvents` then re-reads the status so the
 *   fields an event does not carry (paths, peers, counters) catch up.
 * - `useForChat` — this window's choice to send chat to the connected
 *   machine. Client-side only: the daemon has no opinion about which
 *   upstream a GUI turn should pick, and it is cleared when the connection
 *   goes, because a preference for a machine that is gone is a surprise on
 *   the next send.
 */

import { createEventStore } from './createEventStore';
import type { RemoteEvent } from './transport/types/events';
import type { RemoteStatus } from './transport/types/remote';

export interface RemoteState {
  /** The last status the daemon reported, or `null` before hydration. */
  status: RemoteStatus | null;
  /** Send chat turns to the connected machine rather than a local server. */
  useForChat: boolean;
}

/** A status with nothing on: what a fresh daemon reports. */
export const IDLE_STATUS: RemoteStatus = {
  enabled: false,
  ticket_fingerprint: null,
  pairing_active: false,
  paired: false,
  path: null,
  peers: [],
  mcp_allowed: false,
  tunnelled_requests: 0,
  last_tunnelled_ms: null,
  last_peer: null,
  connected: null,
  stored_ticket_fingerprint: null,
  has_remote_key: false,
};

const INITIAL: RemoteState = { status: null, useForChat: false };

const store = createEventStore<RemoteState>(INITIAL);

/** Replace the status with what the daemon just said. */
export function applyRemoteStatus(status: RemoteStatus): void {
  const prev = store.getState();
  store.setState({
    status,
    // A preference for a machine that is gone does not survive its going.
    useForChat: prev.useForChat && status.connected !== null,
  });
}

/**
 * Move the status forward on an event, without waiting for the re-read.
 *
 * Each arm changes only what the event proves. `remote_connected` carries a
 * port and nothing else, so the connection it writes is a placeholder the
 * next status read replaces — but it is enough for a panel to switch to the
 * connected view now rather than a fetch later.
 */
export function ingestRemoteEvent(evt: RemoteEvent): void {
  const prev = store.getState();
  const status = prev.status ?? IDLE_STATUS;
  switch (evt.type) {
    case 'remote_enabled':
      store.setState({
        ...prev,
        status: {
          ...status,
          enabled: true,
          ticket_fingerprint: evt.ticketFingerprint,
          pairing_active: true,
          paired: false,
        },
      });
      break;
    case 'remote_disabled':
      store.setState({
        ...prev,
        status: {
          ...status,
          enabled: false,
          ticket_fingerprint: null,
          pairing_active: false,
          paired: false,
          path: null,
          peers: [],
        },
      });
      break;
    case 'remote_paired':
      store.setState({
        ...prev,
        status: { ...status, pairing_active: false, paired: true, last_peer: evt.peer },
      });
      break;
    case 'remote_connected':
      store.setState({
        ...prev,
        status: {
          ...status,
          connected: {
            port: evt.port,
            base_url: `http://127.0.0.1:${evt.port}/v1`,
            ticket_fingerprint: status.stored_ticket_fingerprint ?? '',
            path: 'idle',
          },
        },
      });
      break;
    case 'remote_disconnected':
      store.setState({ status: { ...status, connected: null }, useForChat: false });
      break;
  }
}

/** This window's choice to send chat to the connected machine. */
export function setUseRemoteForChat(useForChat: boolean): void {
  const prev = store.getState();
  // Only meaningful while connected; the flag is never left armed for later.
  store.setState({ ...prev, useForChat: useForChat && prev.status?.connected != null });
}

/** Reset (used during cleanup / hot-reload). */
export function resetRemoteState(): void {
  store.setState(INITIAL);
}

/** Read the state outside React — the runtime hook's send path. */
export function getRemoteState(): RemoteState {
  return store.getState();
}

/** React hook — subscribe to the full remote state. */
export function useRemoteState(): RemoteState {
  return store.useStore();
}
