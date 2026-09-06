/**
 * The bridge from the daemon's remote events into the remote registry.
 *
 * Same two pieces of ordering care as the proxy bridge — subscribe before the
 * hydration fetch, drop a fetch a live event overtook — plus the one thing
 * this bridge adds: an event triggers a status re-read, because the events
 * carry a fingerprint or a port and the panel wants the rest.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const subscribeSseEvent = vi.fn();
const getRemoteStatus = vi.fn();
const applyRemoteStatus = vi.fn();
const ingestRemoteEvent = vi.fn();
const resetRemoteState = vi.fn();

vi.mock('../../../src/services/transport/events/sse', () => ({ subscribeSseEvent }));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ getRemoteStatus }),
}));

vi.mock('../../../src/services/remoteRegistry', () => ({
  applyRemoteStatus,
  ingestRemoteEvent,
  resetRemoteState,
}));

async function loadFresh() {
  vi.resetModules();
  return import('../../../src/services/remoteEvents');
}

const STATUS = {
  enabled: true,
  ticket_fingerprint: 'aabbccddeeff',
  pairing_active: true,
  paired: false,
  path: 'idle',
  peers: [],
  mcp_allowed: false,
  tunnelled_requests: 0,
  last_tunnelled_ms: null,
  last_peer: null,
  connected: null,
  stored_ticket_fingerprint: null,
  has_remote_key: false,
};

describe('initRemoteEvents', () => {
  beforeEach(() => {
    subscribeSseEvent.mockReset().mockReturnValue(() => {});
    getRemoteStatus.mockReset().mockResolvedValue(STATUS);
    applyRemoteStatus.mockReset();
    ingestRemoteEvent.mockReset();
    resetRemoteState.mockReset();
  });

  afterEach(() => {
    vi.resetModules();
  });

  it('subscribes to the remote category, then hydrates from the status', async () => {
    const { initRemoteEvents, cleanupRemoteEvents } = await loadFresh();
    initRemoteEvents();

    expect(subscribeSseEvent).toHaveBeenCalledTimes(1);
    expect(subscribeSseEvent).toHaveBeenCalledWith('remote', expect.any(Function));
    await vi.waitFor(() => expect(applyRemoteStatus).toHaveBeenCalledWith(STATUS));

    cleanupRemoteEvents();
    expect(resetRemoteState).toHaveBeenCalled();
  });

  it('an event is ingested at once and followed by a status re-read', async () => {
    const { initRemoteEvents, cleanupRemoteEvents } = await loadFresh();
    initRemoteEvents();
    await vi.waitFor(() => expect(getRemoteStatus).toHaveBeenCalledTimes(1));

    const handler = subscribeSseEvent.mock.calls[0][1] as (evt: unknown) => void;
    handler({ type: 'remote_connected', port: 41234 });

    expect(ingestRemoteEvent).toHaveBeenCalledWith({ type: 'remote_connected', port: 41234 });
    await vi.waitFor(() => expect(getRemoteStatus).toHaveBeenCalledTimes(2));

    cleanupRemoteEvents();
  });

  it('drops a hydration answer that a live event overtook', async () => {
    let resolveStatus: (s: typeof STATUS) => void = () => {};
    getRemoteStatus.mockImplementationOnce(
      () => new Promise<typeof STATUS>((resolve) => (resolveStatus = resolve)),
    );

    const { initRemoteEvents, cleanupRemoteEvents } = await loadFresh();
    initRemoteEvents();

    const handler = subscribeSseEvent.mock.calls[0][1] as (evt: unknown) => void;
    handler({ type: 'remote_enabled', ticketFingerprint: 'aabbccddeeff' });
    resolveStatus({ ...STATUS, enabled: false });
    await vi.waitFor(() => expect(getRemoteStatus).toHaveBeenCalledTimes(2));

    // The stale first answer never landed; only the re-read may.
    expect(applyRemoteStatus).not.toHaveBeenCalledWith({ ...STATUS, enabled: false });

    cleanupRemoteEvents();
  });
});
