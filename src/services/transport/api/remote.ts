/**
 * Remote tunnel API module (ADR 0012).
 *
 * Six calls, two sides. `enable`/`disable`/`getRemoteStatus` are this machine
 * as the desktop: the tunnel in front of its own proxy. `connect`/`disconnect`
 * /`kill` are this machine as the laptop: a loopback port here that is another
 * machine's proxy. Every one goes to the daemon over HTTP — there are no Tauri
 * commands for the tunnel, so web and desktop take the same path.
 */

import { get, post } from './client';
import {
  REMOTE_CONNECT_PATH,
  REMOTE_DISABLE_PATH,
  REMOTE_DISCONNECT_PATH,
  REMOTE_ENABLE_PATH,
  REMOTE_KILL_PATH,
  REMOTE_STATUS_PATH,
} from '../../api/routes';
import type {
  RemoteConnectBody,
  RemoteConnectResponse,
  RemoteEnableBody,
  RemoteEnableResponse,
  RemoteStatus,
} from '../types/remote';

/** The tunnel as the daemon reports it: both sides, fingerprints only. */
export async function getRemoteStatus(): Promise<RemoteStatus> {
  return get<RemoteStatus>(REMOTE_STATUS_PATH);
}

/**
 * Bring the tunnel up in front of this machine's proxy and arm a pairing.
 *
 * The response is the only time the ticket and the pairing code are handed
 * out; a second call while enabled is a 409, not a re-read. Show it once,
 * and let it go when a device pairs or the code expires.
 */
export async function enableRemote(body: Partial<RemoteEnableBody> = {}): Promise<RemoteEnableResponse> {
  return post<RemoteEnableResponse>(REMOTE_ENABLE_PATH, body);
}

/** Take the tunnel down. Idempotent; the ticket is dead from this moment. */
export async function disableRemote(): Promise<RemoteStatus> {
  return post<RemoteStatus>(REMOTE_DISABLE_PATH, {});
}

/**
 * Reach another machine: bind a loopback port here that is its proxy. With
 * `pairing` as `<ticket>-<code>` the code is redeemed for that machine's key
 * and stored; omitted, the last ticket is dialled with the stored key.
 */
export async function connectRemote(body: Partial<RemoteConnectBody> = {}): Promise<RemoteConnectResponse> {
  return post<RemoteConnectResponse>(REMOTE_CONNECT_PATH, body);
}

/** Close the loopback port. Idempotent; the stored pairing stays. */
export async function disconnectRemote(): Promise<RemoteStatus> {
  return post<RemoteStatus>(REMOTE_DISCONNECT_PATH, {});
}

/**
 * Stop the far machine's daemon through the tunnel, then disconnect.
 *
 * A one-way door: nothing brings that daemon back except someone at the
 * machine. The daemon requires the confirmation word in the body, exactly as
 * the proxy route it forwards to does, so this cannot be reached by an
 * accidental empty POST — the caller has already asked the person.
 */
export async function killRemote(): Promise<RemoteStatus> {
  return post<RemoteStatus>(REMOTE_KILL_PATH, { confirm: 'shutdown' });
}
