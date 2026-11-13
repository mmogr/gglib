/**
 * What the Devices place tests read: the remote status's pieces as the
 * daemon sends them, every field present, and an offer as `enable` with
 * `invite` or `invite` answers one. A row's `description` is the words
 * `device_line.rs` writes for it, and a status's `sharing`, `sharing_note`
 * `reach_line` and `reach_note` the words `status_lines.rs` writes.
 */

import type { RemoteDevice, RemoteEnableResponse, RemoteStatus } from '../../../src/services/transport/types/remote';
import { IDLE_STATUS } from '../../../src/services/remoteRegistry';

export const TICKET = 'pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na';

/** The paired machine's fingerprint: its identity, and never on screen. */
export const FINGERPRINT = '3ca82708b995';

/** A device that redeemed its invite and has made requests since. */
export function device(overrides: Partial<RemoteDevice> = {}): RemoteDevice {
  return {
    id: 'dev-0a1b2c3d',
    label: "Matt's phone",
    joined_at: Date.now() - 86_400_000,
    redeemed_at: Date.now() - 86_400_000,
    last_seen: Date.now() - 60_000,
    admitted: true,
    peer: null,
    recorded: true,
    description: 'last seen 1m ago',
    joined: true,
    ...overrides,
  };
}

/** An invite nobody took, as the daemon lists the row it wrote. */
export function invited(id = 'dev-99887766'): RemoteDevice {
  return device({ id, label: undefined, redeemed_at: null, last_seen: null, description: 'invited just now, never joined', joined: false });
}

/** An answer carrying a code; `expires_in_s` from the daemon's clock. */
export function offerAnswer(code: string, overrides: Partial<RemoteEnableResponse> = {}): RemoteEnableResponse {
  return {
    ticket: TICKET,
    code,
    pairing: `${TICKET}-${code}`,
    expires_in_s: 120,
    mcp_servers: [],
    already_up: false,
    ...overrides,
  };
}

/** This Mac shared, with nothing else said. */
export const SHARED: RemoteStatus = {
  ...IDLE_STATUS,
  enabled: true,
  remote_enabled: true,
  ticket_fingerprint: 'aa11bb22cc33',
  sharing: 'Sharing · 0 paired', sharing_note: '', reach_line: '',
};

/** Joined to `desk`, reached directly. */
export const DESK: RemoteStatus = {
  ...IDLE_STATUS,
  connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: FINGERPRINT, path: 'direct', away_for_s: null },
  reach: 'connected',
  stored_ticket_fingerprint: FINGERPRINT,
  paired_name: 'desk',
  has_remote_key: true,
  sharing: 'Not sharing', sharing_note: '', reach_line: 'desk · Connected · direct',
};

/** Whether the paired machine's fingerprint appears anywhere in the page: text or attribute. */
export function fingerprintShown(): boolean {
  return document.body.innerHTML.includes(FINGERPRINT);
}
