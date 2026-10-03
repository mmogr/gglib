/**
 * The remote registry: what each event proves, and nothing more.
 *
 * The events are thin on purpose (a fingerprint, a port), so an arm that
 * wrote more than its event carries would be inventing state. And what the
 * page holds of the paired machine — a far row, a pick, a chat — is held for
 * that machine by its fingerprint: it outlives a disconnection, and never
 * follows to another machine, whose same id is another model.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import {
  IDLE_STATUS,
  applyRemoteStatus,
  getRemoteState,
  ingestRemoteEvent,
  resetRemoteState,
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
    expect(getRemoteState()).toEqual({ status: null });
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
