/**
 * When a pairing code on offer ends, from the status reads alone.
 *
 * The daemon leaves gaps a read can land in: a redemption frees the code
 * before it records the pairing, and an expiry lets the code go a moment
 * before this side's clock. A rule that ended the offer in either gap would
 * call a pairing or an expiry a withdrawal, and the app would say nothing.
 */

import { describe, it, expect } from 'vitest';

import { offerFrom, settleOffer, type PairingOffer } from '../../../src/services/remoteOffer';
import { IDLE_STATUS } from '../../../src/services/remoteRegistry';
import type { RemoteDevice, RemoteStatus } from '../../../src/services/transport/types/remote';

const DEVICE = 'dev-99887766';

/** The row an invite writes for `DEVICE`, joined or not. */
function row(joined: boolean): RemoteDevice {
  return {
    id: DEVICE,
    joined_at: 1,
    redeemed_at: joined ? 2 : null,
    last_seen: null,
    admitted: true,
    peer: null,
    recorded: true,
    description: joined ? 'joined just now' : 'invited just now, never joined',
    joined,
  };
}

const offer: PairingOffer = {
  pairing: 'ticket-119284',
  code: '119284',
  device: DEVICE,
  expiresAt: 120_000,
  rowSeen: false,
  liveSeen: false,
  codeGone: false,
};
/** Its row listed and its code read live: an offer the status has caught up with. */
const seen: PairingOffer = { ...offer, rowSeen: true, liveSeen: true };

/** A serving status, with `patch` applied. */
function up(patch: Partial<RemoteStatus> = {}): RemoteStatus {
  return { ...IDLE_STATUS, enabled: true, ...patch };
}

describe('offerFrom', () => {
  it('holds the code with its deadline, counted from when the answer arrived', () => {
    const held = offerFrom(
      { ticket: 't', pairing: 't-119284', code: '119284', expires_in_s: 120, device: DEVICE, mcp_servers: [], already_up: false },
      1_000,
    );
    expect(held).toEqual({ pairing: 't-119284', code: '119284', device: DEVICE, expiresAt: 121_000, rowSeen: false, liveSeen: false, codeGone: false });
  });

  it('holds nothing when the answer carries no code', () => {
    expect(offerFrom({ ticket: 't', mcp_servers: [], already_up: false }, 1_000)).toBeNull();
  });
});

describe('settleOffer', () => {
  it('ends as paired when paired turns true', () => {
    expect(settleOffer(seen, up(), up({ paired: true }))).toEqual({ offer: null, ended: 'paired' });
  });

  it('a paired left over from an earlier device does not end a fresh code', () => {
    const still = settleOffer(offer, up({ paired: true }), up({ paired: true, pairing_active: true }));
    expect(still.ended).toBeUndefined();
    expect(still.offer).not.toBeNull();
  });

  it('ends as paired when the offered row reads joined, whatever paired says', () => {
    const before = up({ paired: true, devices: [row(false)] });
    expect(settleOffer(seen, before, up({ paired: true, devices: [row(true)] }))).toEqual({
      offer: null,
      ended: 'paired',
    });
  });

  it('ends as tunnel down when the tunnel goes', () => {
    expect(settleOffer(seen, up({ devices: [row(false)] }), { ...IDLE_STATUS, devices: [row(false)] })).toEqual({
      offer: null,
      ended: 'tunnel_down',
    });
  });

  it('a pairing read in the same move as the tunnel going is a pairing', () => {
    expect(settleOffer(seen, up(), { ...IDLE_STATUS, paired: true }).ended).toBe('paired');
  });

  it('ends as withdrawn once a row it saw is gone', () => {
    expect(settleOffer(seen, up({ devices: [row(false)] }), up())).toEqual({ offer: null, ended: 'withdrawn' });
  });

  it('a row it never saw is not a withdrawal: the read came before the answer', () => {
    expect(settleOffer(offer, up(), up())).toEqual({ offer });
  });

  it('the first read that lists the row and the live code marks both seen', () => {
    expect(settleOffer(offer, up(), up({ pairing_active: true, devices: [row(false)] }))).toEqual({ offer: seen });
  });

  it('a code gone with its row still there, unjoined, is kept and marked gone', () => {
    // The redemption gap, the expiry gap, or a burned code: the daemon no
    // longer holds the code, nothing says paired, and the row is still
    // there. The registry's grace timer tells them apart.
    const before = up({ pairing_active: true, devices: [row(false)] });
    expect(settleOffer(seen, before, up({ devices: [row(false)] }))).toEqual({
      offer: { ...seen, codeGone: true },
    });
  });

  it('a code read live again is no longer gone', () => {
    const gone = { ...seen, codeGone: true };
    expect(settleOffer(gone, up(), up({ pairing_active: true, devices: [row(false)] }))).toEqual({
      offer: seen,
    });
  });

  it('a code never read live is not gone: the read came before the answer', () => {
    expect(settleOffer(offer, up(), up({ devices: [row(false)] })).offer?.codeGone).toBe(false);
  });

  it('an offer naming no device is marked gone the same way, and ends only by pairing or the tunnel', () => {
    const anonymous: PairingOffer = { ...offer, device: undefined, liveSeen: true };
    const settled = settleOffer(anonymous, up({ pairing_active: true }), up());
    expect(settled.ended).toBeUndefined();
    expect(settled.offer?.codeGone).toBe(true);
    expect(settleOffer(anonymous, up(), up({ paired: true })).ended).toBe('paired');
  });

  it('nothing on offer stays nothing', () => {
    expect(settleOffer(null, up(), up({ paired: true }))).toEqual({ offer: null });
  });
});
