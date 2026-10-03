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
  paired_name: null,
  has_remote_key: false,
  remote_enabled: false,
  identity_path: null,
  devices: [],
};

/**
 * What the paired machine is shown as when it has given no name: the words
 * the daemon and the CLI use (`UNNAMED_PAIRED` in gglib-core). Never its
 * fingerprint, which is its identity and is not shown.
 */
export const UNNAMED_PAIRED = 'the paired machine';

/** The name the paired machine is shown by, from the stored pairing. */
export function pairedName(status: RemoteStatus | null): string {
  return status?.paired_name || UNNAMED_PAIRED;
}

export const INITIAL: RemoteState = { status: null };
