/**
 * The bridge from the daemon's remote events into the remote registry.
 *
 * Same two pieces of ordering care as the proxy bridge — subscribe before the
 * hydration fetch, drop a fetch a live event overtook — plus the one thing
 * this bridge adds: an event triggers a status re-read, because the events
 * carry a fingerprint or a port and the panel wants the rest.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';

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

/**
 * Annotated, which it was not: this literal had gone stale twice over,
 * missing `remote_enabled` and `identity_path` long after the Rust grew
 * them, because nothing typechecked it against the shape it claims to be.
 * A test fixture that has drifted from the real payload is testing the
 * wrong thing quietly. `tsc` owns it now.
 */
const STATUS: RemoteStatus = {
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
  remote_enabled: true,
  identity_path: '/home/matt/.gglib/remote_identity',
  devices: [],
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

  it('a re-read overtaken by an event is retried, not reported as failed', async () => {
    const { initRemoteEvents, refreshRemoteStatus, cleanupRemoteEvents } = await loadFresh();
    initRemoteEvents();
    await vi.waitFor(() => expect(getRemoteStatus).toHaveBeenCalledTimes(1));

    let resolveStatus: (s: typeof STATUS) => void = () => {};
    getRemoteStatus.mockImplementationOnce(
      () => new Promise<typeof STATUS>((resolve) => (resolveStatus = resolve)),
    );
    const reread = refreshRemoteStatus();

    const handler = subscribeSseEvent.mock.calls[0][1] as (evt: unknown) => void;
    handler({ type: 'remote_back', port: 41234 });
    resolveStatus(STATUS);

    // The read worked; an event merely overtook it. Reporting that as a
    // failure told a forget its list "could not be re-read" while the event's
    // own read was already landing.
    await expect(reread).resolves.toBe(true);
    // Hydration, this read, the event's read, and the retry.
    expect(getRemoteStatus).toHaveBeenCalledTimes(4);

    cleanupRemoteEvents();
  });

  it('refreshRemoteStatus answers true once applied, and false rather than rejecting when the read fails', async () => {
    // The contract a forget relies on to decide whether it may say it is done.
    // The component tests mock it, so this is the one place it is held to it.
    const { refreshRemoteStatus } = await loadFresh();
    await expect(refreshRemoteStatus()).resolves.toBe(true);
    expect(applyRemoteStatus).toHaveBeenCalledWith(STATUS);

    getRemoteStatus.mockRejectedValueOnce(new Error('daemon gone'));
    await expect(refreshRemoteStatus()).resolves.toBe(false);
  });
});
