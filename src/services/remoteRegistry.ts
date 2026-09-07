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
 * - `chatModel` — the far machine's name for the model those turns ask for.
 *   Also client-side only, and mandatory on that path: this machine's
 *   default is deliberately not sent, because the far one may not have it
 *   (`docs/remote.md`).
 * - `chatRequestedAt` — a request from the Remote panel to put the chat
 *   screen on screen, aimed at the far machine. The panel is mounted in the
 *   model library's header and the chat screen replaces the whole Model
 *   Control Center, so neither can reach the other by props; this store is
 *   the seam between them. It is one-shot: the page clears it as it opens,
 *   so asking twice opens twice.
 */

import { createEventStore } from './createEventStore';
import type { RemoteEvent } from './transport/types/events';
import type { RemoteStatus } from './transport/types/remote';

export interface RemoteState {
  /** The last status the daemon reported, or `null` before hydration. */
  status: RemoteStatus | null;
  /** Send chat turns to the connected machine rather than a local server. */
  useForChat: boolean;
  /**
   * The model name those turns carry, as the far machine spells it.
   *
   * Empty until someone types one. It outlives a disconnection on purpose:
   * the ordinary reconnection is the stored ticket dialled again, the same
   * machine serving the same models, and retyping the name each time buys
   * nothing. It cannot be sent to a machine nobody chose, because it is only
   * read while `useForChat` is on and that does not survive the connection
   * going.
   */
  chatModel: string;
  /**
   * When the panel last asked for the chat screen, or `null` for not asked.
   *
   * A timestamp rather than a boolean because it is an event, not a mode:
   * the page reacts to the value changing and clears it again, so a second
   * request after the first was served is a second distinct value.
   */
  chatRequestedAt: number | null;
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

const INITIAL: RemoteState = {
  status: null,
  useForChat: false,
  chatModel: '',
  chatRequestedAt: null,
};

const store = createEventStore<RemoteState>(INITIAL);

/** Replace the status with what the daemon just said. */
export function applyRemoteStatus(status: RemoteStatus): void {
  const prev = store.getState();
  store.setState({
    ...prev,
    status,
    // A preference for a machine that is gone does not survive its going,
    // and neither does a request to open a chat screen against it.
    useForChat: prev.useForChat && status.connected !== null,
    chatRequestedAt: status.connected !== null ? prev.chatRequestedAt : null,
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
      store.setState({
        ...prev,
        status: { ...status, connected: null },
        useForChat: false,
        chatRequestedAt: null,
      });
      break;
  }
}

/** This window's choice to send chat to the connected machine. */
export function setUseRemoteForChat(useForChat: boolean): void {
  const prev = store.getState();
  // Only meaningful while connected; the flag is never left armed for later.
  store.setState({ ...prev, useForChat: useForChat && prev.status?.connected != null });
}

/**
 * The model name chat turns should ask the connected machine for.
 *
 * Stored as typed — trimming and the "you named none" refusal both live on
 * the send path, so what the field shows is what the panel was given.
 */
export function setRemoteChatModel(chatModel: string): void {
  store.setState({ ...store.getState(), chatModel });
}

/**
 * Ask for the chat screen, pointed at the connected machine.
 *
 * Refused while nothing is connected: the screen would have no upstream and
 * the first send would be the place the user found out.
 */
export function requestRemoteChat(): void {
  const prev = store.getState();
  if (prev.status?.connected == null) return;
  store.setState({ ...prev, chatRequestedAt: Date.now() });
}

/** Served: the page has the request and the next one must be distinct. */
export function clearRemoteChatRequest(): void {
  store.setState({ ...store.getState(), chatRequestedAt: null });
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
