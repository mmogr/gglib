/**
 * The remote registry: what each event proves, and nothing more.
 *
 * The events are thin on purpose (a fingerprint, a port), so an arm that
 * wrote more than its event carries would be inventing state. And what the
 * page holds of the paired machine — a far row, a pick, a chat — is held for
 * that machine by its fingerprint: it outlives a disconnection, and never
 * follows to another machine, whose same id is another model.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

import {
  IDLE_STATUS,
  applyRemoteStatus,
  clearOffer,
  getRemoteState,
  ingestRemoteEvent,
  resetRemoteState,
  showOffer,
  stillPaired,
} from '../../../src/services/remoteRegistry';
import type { Machine } from '../../../src/types/generated/Machine';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: '3ca82708b995',
  path: 'direct',
  // Here rather than away: everything below is about which machine a held
  // far model follows, and a machine that is answering is the case those
  // rules are written for. The away arm has its own tests.
  away_for_s: null,
};

/** A second machine: the same tunnel, a different catalog. */
const otherMachine = { ...connected, port: 41235, ticket_fingerprint: 'ffee11223344' };

describe('remoteRegistry', () => {
  beforeEach(() => resetRemoteState());

  it('starts with no status', () => {
    expect(getRemoteState()).toEqual({ status: null, offer: null, ended: null });
  });

  /**
   * The event carries a fingerprint and nothing else — in particular it does
   * not say whether a code was offered, and an `enable` that was not asked to
   * invite offers none. That is now the ordinary way a paired machine comes
   * back, so assuming a live code here would tell the panel to disable its
   * Invite button until the status re-read that follows every event landed,
   * and for good if that read failed.
   */
  it('remote_enabled turns the serve side on and claims nothing about a code', () => {
    ingestRemoteEvent({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
    const { status } = getRemoteState();
    expect(status?.enabled).toBe(true);
    expect(status?.ticket_fingerprint).toBe('aabbccddeeff');
    expect(status?.pairing_active).toBe(false);
    expect(status?.paired).toBe(false);
  });

  it('remote_paired spends the code; remote_disabled clears the session', () => {
    // Seeded through a status rather than an `enable` event, which no longer
    // claims a code is live.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    expect(getRemoteState().status).toMatchObject({
      pairing_active: false,
      paired: true,
      last_peer: '0123456789ab',
    });

    ingestRemoteEvent({ type: 'remote_disabled' });
    expect(getRemoteState().status).toMatchObject({
      enabled: false,
      ticket_fingerprint: null,
      pairing_active: false,
      paired: false,
      peers: [],
    });
  });

  it('remote_joined writes a placeholder connection the status read replaces', () => {
    ingestRemoteEvent({ type: 'remote_joined', port: 41234 });
    expect(getRemoteState().status?.connected).toMatchObject({
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
    });

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().status?.connected).toEqual(connected);
  });

  it('a machine that goes away keeps the connection', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });

    ingestRemoteEvent({ type: 'remote_away', port: 41234 });

    // The port is still bound and still dialling, so nothing about this is a
    // disconnection: the panel says away, and what is held for that machine
    // waits for it rather than being cleared.
    expect(getRemoteState().status?.connected?.away_for_s).toBe(0);
    expect(getRemoteState().status?.connected?.port).toBe(41234);
    expect(stillPaired(first, getRemoteState().status)).toBe(true);
  });

  it('a machine that answers again is here', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    ingestRemoteEvent({ type: 'remote_away', port: 41234 });

    ingestRemoteEvent({ type: 'remote_back', port: 41234 });

    expect(getRemoteState().status?.connected?.away_for_s).toBeNull();
  });

  it('an away event with nothing connected changes nothing', () => {
    // The watcher is the only source of these and it only runs behind a
    // live connection, but a status read can race one in and leave the
    // store empty; a spread onto `null` would invent a connection.
    ingestRemoteEvent({ type: 'remote_away', port: 41234 });
    expect(getRemoteState().status).toBeNull();
  });
});

/** An invite's answer: a code for `dev-99887766`, live for `s` seconds. */
function invited(s = 120) {
  return {
    ticket: 't',
    pairing: 't-119284',
    code: '119284',
    expires_in_s: s,
    device: 'dev-99887766',
    mcp_servers: [],
    already_up: true,
  };
}

/** The row `invited` writes, not joined yet. */
const unjoined = {
  id: 'dev-99887766',
  joined_at: 1,
  redeemed_at: null,
  last_seen: null,
  admitted: true,
  peer: null,
  recorded: true,
  description: 'invited just now, never joined',
  joined: false,
};

describe('the pairing code on offer', () => {
  beforeEach(() => {
    resetRemoteState();
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
  });
  afterEach(() => vi.useRealTimers());

  it('ends as expired at its deadline, numbered', () => {
    vi.useFakeTimers();
    showOffer(invited(120));
    vi.advanceTimersByTime(119_000);
    expect(getRemoteState().offer?.code).toBe('119284');
    vi.advanceTimersByTime(1_000);
    expect(getRemoteState().offer).toBeNull();
    expect(getRemoteState().ended).toMatchObject({ reason: 'expired', device: 'dev-99887766' });
  });

  it('remote_paired ends it as paired, even over a paired left from an earlier device', () => {
    vi.useFakeTimers();
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, paired: true });
    showOffer(invited());
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    expect(getRemoteState().offer).toBeNull();
    expect(getRemoteState().ended?.reason).toBe('paired');
    // And neither the clock nor a second signal ends it again.
    const seq = getRemoteState().ended?.seq;
    vi.advanceTimersByTime(120_000);
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    expect(getRemoteState().ended?.seq).toBe(seq);
  });

  it('remote_disabled ends it as the tunnel going, and the clock stops', () => {
    vi.useFakeTimers();
    showOffer(invited());
    ingestRemoteEvent({ type: 'remote_disabled' });
    expect(getRemoteState().ended?.reason).toBe('tunnel_down');
    vi.advanceTimersByTime(120_000);
    expect(getRemoteState().ended?.reason).toBe('tunnel_down');
  });

  it('clearing it ends nothing, and the clock stops', () => {
    vi.useFakeTimers();
    showOffer(invited());
    clearOffer();
    expect(getRemoteState().offer).toBeNull();
    vi.advanceTimersByTime(120_000);
    expect(getRemoteState().ended).toBeNull();
  });

  it('a reset drops it, and the clock stops', () => {
    vi.useFakeTimers();
    showOffer(invited());
    resetRemoteState();
    vi.advanceTimersByTime(120_000);
    expect(getRemoteState()).toEqual({ status: null, offer: null, ended: null });
  });

  it('a code replaced by a fresh one is not expired by the old one\'s clock', () => {
    vi.useFakeTimers();
    showOffer(invited(10));
    showOffer({ ...invited(120), code: '222222' });
    vi.advanceTimersByTime(10_000);
    expect(getRemoteState().offer?.code).toBe('222222');
    expect(getRemoteState().ended).toBeNull();
  });

  it('a code gone early, its row still unjoined, lapses after the grace', () => {
    // A burned code, or one a restart dropped: no event, no pairing.
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(2_999);
    expect(getRemoteState().offer).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(getRemoteState().offer).toBeNull();
    expect(getRemoteState().ended?.reason).toBe('lapsed');
  });

  it('a row listed before its code is live still lapses once the code goes', () => {
    // The daemon writes the roster row before it makes the code live, so a
    // read can list the row with no code live yet.
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(3_000);
    expect(getRemoteState().ended?.reason).toBe('lapsed');
  });

  it('the grace counts from the latest time the code went, not the first', () => {
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(1_000);
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    vi.advanceTimersByTime(1_900);
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(200);
    expect(getRemoteState().offer?.code).toBe('119284');
  });

  it('a fresh code gets the whole grace, whatever the last one left running', () => {
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    clearOffer();
    vi.advanceTimersByTime(1_000);
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(2_500);
    expect(getRemoteState().offer?.code).toBe('119284');
  });

  it('a remote_paired inside the grace is a pairing, and nothing lapses after', () => {
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    ingestRemoteEvent({ type: 'remote_paired', peer: '0123456789ab' });
    const seq = getRemoteState().ended?.seq;
    vi.advanceTimersByTime(10_000);
    expect(getRemoteState().ended).toMatchObject({ reason: 'paired', seq });
  });

  it('a code the daemon let go just before its deadline is called expired, not lapsed', () => {
    // The daemon's clock leads this side's, so it lets go first.
    vi.useFakeTimers();
    showOffer(invited(4));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    vi.advanceTimersByTime(2_500);
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    vi.advanceTimersByTime(3_000);
    expect(getRemoteState().ended?.reason).toBe('expired');
  });

  it('a code read live again inside the grace is kept', () => {
    vi.useFakeTimers();
    showOffer(invited(120));
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [unjoined] });
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [unjoined] });
    vi.advanceTimersByTime(10_000);
    expect(getRemoteState().offer?.code).toBe('119284');
    expect(getRemoteState().ended).toBeNull();
  });

  it('an answer with no code drops the one held', () => {
    showOffer(invited());
    showOffer({ ticket: 't', mcp_servers: [], already_up: true });
    expect(getRemoteState().offer).toBeNull();
    expect(getRemoteState().ended).toBeNull();
  });

  it('a status read moves it on: its row seen, then forgotten, is a withdrawal', () => {
    showOffer(invited());
    const row = {
      id: 'dev-99887766',
      joined_at: 1,
      redeemed_at: null,
      last_seen: null,
      admitted: true,
      peer: null,
      recorded: true,
      description: 'invited just now, never joined',
      joined: false,
    };
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true, devices: [row] });
    expect(getRemoteState().offer?.rowSeen).toBe(true);
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
    expect(getRemoteState().ended?.reason).toBe('withdrawn');
  });
});

/** The machine `connected` is, as a far model held for it names it. */
const first: Machine = { kind: 'paired', fingerprint: '3ca82708b995' };

describe('stillPaired', () => {
  beforeEach(() => resetRemoteState());

  it('holds for the machine connected to', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(stillPaired(first, getRemoteState().status)).toBe(true);
  });

  it('holds across a disconnection, which is not a new machine', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    ingestRemoteEvent({ type: 'remote_disconnected' });
    applyRemoteStatus(IDLE_STATUS);
    expect(stillPaired(first, getRemoteState().status)).toBe(true);
  });

  it('holds while the placeholder connection names nobody yet', () => {
    ingestRemoteEvent({ type: 'remote_joined', port: 41234 });
    expect(getRemoteState().status?.connected?.ticket_fingerprint).toBe('');
    expect(stillPaired(first, getRemoteState().status)).toBe(true);
  });

  it('drops once another machine answers, with or without a disconnection between', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(stillPaired(first, getRemoteState().status)).toBe(false);

    resetRemoteState();
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    ingestRemoteEvent({ type: 'remote_joined', port: 41235 });
    applyRemoteStatus({ ...IDLE_STATUS, connected: otherMachine });
    expect(stillPaired(first, getRemoteState().status)).toBe(false);
  });

  it('never holds a model of this machine as the paired one', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(stillPaired({ kind: 'local' }, getRemoteState().status)).toBe(false);
  });
});
