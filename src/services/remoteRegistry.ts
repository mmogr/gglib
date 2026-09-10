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
 * - `chatModel` — the far machine's name for the model those turns ask for,
 *   and `chatModelPeer`, the peer it was typed for. Also client-side only,
 *   and mandatory on that path: this machine's default is deliberately not
 *   sent, because the far one may not have it (`docs/remote.md`).
 * - `chatRequestedAt` — a request from the Remote panel to put the chat
 *   screen on screen, aimed at the far machine. The panel is mounted in the
 *   model library's header and the chat screen replaces the whole Model
 *   Control Center, so neither can reach the other by props; this store is
 *   the seam between them. It is one-shot: the page clears it as it opens,
 *   so asking twice opens twice.
 *
 * The last three are all aimed at a particular machine, so all three are
 * reconciled against who is actually there: a different peer drops the
 * routing choice, the pending request and the model name together.
 */

import { createEventStore } from './createEventStore';
import type { RemoteEvent } from './transport/types/events';
import type { RemoteStatus } from './transport/types/remote';
import { IDLE_STATUS, INITIAL, type RemoteState } from './remoteRegistryState';

// The shape moved to `remoteRegistryState.ts`; the registry stays the seam
// every caller already imports from, so it hands both on rather than making
// eight components learn a second module name.
export { IDLE_STATUS, type RemoteState };

const store = createEventStore<RemoteState>(INITIAL);

/**
 * The peer a status read reports, or `null` for nobody knowable.
 *
 * `''` is the placeholder connection `ingestRemoteEvent` writes, which knows
 * a port and nothing else: not an identity, never to be compared as one.
 */
function peerOf(status: RemoteStatus | null): string | null {
  return status?.connected?.ticket_fingerprint || null;
}

/**
 * The model name, reconciled against the peer now on the other end.
 *
 * Keeping the name across the same peer's reconnection is the point of
 * keeping it at all; carrying it to a *different* one is how a pre-filled
 * field earns `404 Model 'qwen3' not found` from a working tunnel.
 *
 * Three cases, and only the last one clears:
 *
 * - Nothing connected decides nothing. A disconnection is not a new peer,
 *   and the name outlives it.
 * - A name with no peer recorded has none yet, so the first to answer adopts
 *   it. The state that reaches here in the product is a name typed against
 *   the placeholder connection, before the status read names who answered;
 *   adoption is what makes that window safe, where stamping the placeholder's
 *   fingerprint would attribute the name to whoever was dialled *last*.
 * - Anyone else is a different catalog, and the name does not follow.
 */
function reconcileChatModel(
  prev: RemoteState,
  peer: string | null,
): Pick<RemoteState, 'chatModel' | 'chatModelPeer'> {
  if (peer === null) return { chatModel: prev.chatModel, chatModelPeer: prev.chatModelPeer };
  const samePeer = prev.chatModelPeer === null || prev.chatModelPeer === peer;
  return { chatModel: samePeer ? prev.chatModel : '', chatModelPeer: peer };
}

/** Replace the status with what the daemon just said. */
export function applyRemoteStatus(status: RemoteStatus): void {
  const prev = store.getState();
  const peer = peerOf(status);
  const prevPeer = peerOf(prev.status);
  // A different peer is as much a reason to drop the routing choice and a
  // pending chat request as a disconnection is: neither was decided about
  // whoever is there now. This arm covers the swap a status read is the first
  // news of; a dial whose event arrived is caught by `remote_connected`,
  // because the placeholder erases the peer this would have compared against.
  // Read from the status rather than the name's attribution — the box can be
  // ticked with the field empty, and that choice is no more transferable.
  const stillAimed = peer !== null && (prevPeer === null || prevPeer === peer);
  store.setState({
    ...prev,
    status,
    useForChat: prev.useForChat && stillAimed,
    chatRequestedAt: stillAimed ? prev.chatRequestedAt : null,
    ...reconcileChatModel(prev, peer),
  });
}

/**
 * Move the status forward on an event, without waiting for the re-read.
 *
 * Each arm changes only what the event proves. `remote_connected` carries a
 * port and nothing else, so the connection it writes is a placeholder the
 * next status read replaces — but it is enough for a panel to switch to the
 * connected view now rather than a fetch later.
 *
 * The placeholder names no peer, deliberately. It used to borrow
 * `stored_ticket_fingerprint` — the ticket a bare `connect` would dial,
 * which on a dial to a *new* machine is the old one's, so anything comparing
 * it would judge the peer by whoever was reached last.
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
            ticket_fingerprint: '',
            path: 'idle',
            away_for_s: null,
          },
        },
        // Only a fresh dial emits this, so anything already armed was armed
        // for the connection this one replaces, and the status read cannot be
        // left to notice: the placeholder has erased the peer it would have
        // compared against. `chatModel` is deliberately not in that list —
        // these two are choices about a connection, which a new one voids,
        // while the name is a fact about a catalog and the usual dial is the
        // same desktop again. Clearing it here would empty the field on every
        // reconnection; `applyRemoteStatus` reconciles it by identity instead.
        useForChat: false,
        chatRequestedAt: null,
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
    // The port stays bound either way; these only change what is said
    // about the machine behind it. The status read that follows carries the
    // exact figure; `0` here is "just now".
    case 'remote_away':
      if (status.connected) {
        store.setState({
          ...prev,
          status: { ...status, connected: { ...status.connected, away_for_s: 0 } },
        });
      }
      break;
    case 'remote_back':
      if (status.connected) {
        store.setState({
          ...prev,
          status: { ...status, connected: { ...status.connected, away_for_s: null } },
        });
      }
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
 *
 * The peer is recorded with it, and `peerOf` reads the placeholder's `''` as
 * nobody: a name typed before the status read is left unattributed for that
 * read to adopt. Guessing beats not knowing only if the guess is informed,
 * and the sole fingerprint in that window is whoever was dialled last.
 */
export function setRemoteChatModel(chatModel: string): void {
  const prev = store.getState();
  store.setState({ ...prev, chatModel, chatModelPeer: peerOf(prev.status) });
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
