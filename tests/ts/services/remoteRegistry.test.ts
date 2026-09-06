/**
 * The remote registry: what each event proves, and nothing more.
 *
 * The events are thin on purpose (a fingerprint, a port), so an arm that
 * wrote more than its event carries would be inventing state. And the
 * use-for-chat choice is a preference for a machine; when the machine goes,
 * so does the preference — otherwise the next send goes somewhere the person
 * did not mean.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import {
  IDLE_STATUS,
  applyRemoteStatus,
  getRemoteState,
  ingestRemoteEvent,
  resetRemoteState,
  setUseRemoteForChat,
} from '../../../src/services/remoteRegistry';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: '3ca82708b995',
  path: 'direct',
};

describe('remoteRegistry', () => {
  beforeEach(() => resetRemoteState());

  it('starts with no status and no chat preference', () => {
    expect(getRemoteState()).toEqual({ status: null, useForChat: false });
  });

  it('remote_enabled turns the serve side on with the fingerprint and a live code', () => {
    ingestRemoteEvent({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
    const { status } = getRemoteState();
    expect(status?.enabled).toBe(true);
    expect(status?.ticket_fingerprint).toBe('aabbccddeeff');
    expect(status?.pairing_active).toBe(true);
    expect(status?.paired).toBe(false);
  });

  it('remote_paired spends the code; remote_disabled clears the session', () => {
    ingestRemoteEvent({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
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

  it('remote_connected writes a placeholder connection the status read replaces', () => {
    ingestRemoteEvent({ type: 'remote_connected', port: 41234 });
    expect(getRemoteState().status?.connected).toMatchObject({
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
    });

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    expect(getRemoteState().status?.connected).toEqual(connected);
  });

  it('use-for-chat needs a connection and does not outlive it', () => {
    setUseRemoteForChat(true);
    expect(getRemoteState().useForChat).toBe(false);

    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    expect(getRemoteState().useForChat).toBe(true);

    ingestRemoteEvent({ type: 'remote_disconnected' });
    expect(getRemoteState()).toMatchObject({ useForChat: false });
    expect(getRemoteState().status?.connected).toBeNull();
  });

  it('a status read that shows no connection also drops the preference', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    applyRemoteStatus(IDLE_STATUS);
    expect(getRemoteState().useForChat).toBe(false);
  });
});
