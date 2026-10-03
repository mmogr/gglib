/**
 * Remote Tunnel State Registry (ADR 0012)
 *
 * Event-driven store for the tunnel, both sides, on the same
 * `createEventStore` pattern as `proxyRegistry`. It keeps `status`, the
 * daemon's `RemoteStatus`, whole. Events move it forward the moment they
 * arrive; `remoteEvents` then re-reads the status so the fields an event does
 * not carry (paths, peers, counters) catch up.
 *
 * What the page holds of the paired machine — its rows in the library, a far
 * row picked, a chat open on a far model — is held by that machine's
 * fingerprint, and `stillPaired` says, against the status now, whether it
 * still holds.
 */

import { createEventStore } from './createEventStore';
import type { Machine } from '../types/generated/Machine';
import type { RemoteEvent } from './transport/types/events';
import type { RemoteStatus } from './transport/types/remote';
import { IDLE_STATUS, INITIAL, UNNAMED_PAIRED, pairedName, type RemoteState } from './remoteRegistryState';

// The shape lives in `remoteRegistryState.ts`; the registry stays the seam
// every caller imports from, so it hands the shape, the idle status and the
// paired machine's name on rather than making eight components learn a
// second module name.
export { IDLE_STATUS, UNNAMED_PAIRED, pairedName, type RemoteState };

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
 * Whether something held for `machine` — a far row, a far pick, a chat open
 * on a far model — still holds against `status`.
 *
 * Holding a far model past that machine's reconnection is the point of
 * holding it at all; carrying it to a *different* machine is how an id picked
 * from one machine's list reaches another's, where the same number is another
 * model.
 *
 * Three cases, and only the last one drops it:
 *
 * - Nothing connected decides nothing. A disconnection is not a new peer,
 *   and what was held for that machine outlives it.
 * - No peer named yet is not a new peer either. The state that reaches here
 *   in the product is the placeholder connection, before the status read
 *   names who answered; its `''` is nobody, never a machine to compare.
 * - Anyone else is a different catalog, and nothing follows to it. A ref to
 *   this machine is not the paired machine's, and never holds here.
 */
export function stillPaired(machine: Machine, status: RemoteStatus | null): boolean {
  if (machine.kind !== 'paired') return false;
  const peer = peerOf(status);
  return peer === null || peer === machine.fingerprint;
}

/** Replace the status with what the daemon just said. */
export function applyRemoteStatus(status: RemoteStatus): void {
  store.setState({ ...store.getState(), status });
}

/**
 * Move the status forward on an event, without waiting for the re-read.
 *
 * Each arm changes only what the event proves. `remote_joined` carries a
 * port and nothing else, so the connection it writes is a placeholder the
 * next status read replaces — but it is enough for a panel to switch to the
 * connected view now rather than a fetch later.
 *
 * The placeholder names no peer, deliberately. It used to borrow
 * `stored_ticket_fingerprint` — the ticket a bare `join` would dial,
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
          // Not `pairing_active: true`. The event carries only a fingerprint
          // and says nothing about a code, and an `enable` that was not asked
          // to invite offers none — which is the ordinary case for bringing a
          // paired machine back. Claiming one is live here would disable the
          // Invite button until the status re-read that follows every event
          // landed, and leave it disabled for good if that read failed.
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
    case 'remote_joined':
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
          // The name held is the last status read's, which on a join to
          // another machine is the one this join replaced. Like the
          // connection, the name is left to the read that follows.
          paired_name: null,
        },
      });
      break;
    case 'remote_disconnected':
      store.setState({
        ...prev,
        status: { ...status, connected: null },
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
