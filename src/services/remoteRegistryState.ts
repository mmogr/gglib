/**
 * What the remote registry holds, and what an empty tunnel looks like.
 *
 * Split out of `remoteRegistry.ts` when the shape and the reducer together
 * crossed the 300-line budget `scripts/check_file_complexity.sh` allows.
 * The state is a description and the registry is behaviour, so the seam was
 * already there; a component that only needs the shape now imports the
 * shape.
 */

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
   * The peer `chatModel` was typed for, by ticket fingerprint.
   *
   * A model name is only a name in one catalog, so this is what lets the
   * field survive that peer coming back without following the user to a
   * different one. `null` means no peer is known for what the field holds —
   * nothing connected when it was typed, or only the placeholder — and the
   * next status read adopts it. Meaningless while `chatModel` is empty, which
   * is the one case nothing reads it: stamping a peer onto no name is free.
   *
   * A fingerprint is a *serve session*, not a machine: the far side mints a
   * fresh identity on every `remote enable`, and the old ticket died with the
   * old session, so that is the right granularity anyway.
   */
  chatModelPeer: string | null;
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

export const INITIAL: RemoteState = {
  status: null,
  useForChat: false,
  chatModel: '',
  chatModelPeer: null,
  chatRequestedAt: null,
};
