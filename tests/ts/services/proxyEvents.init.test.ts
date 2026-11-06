/**
 * The bridge from the daemon's proxy events into the proxy registry.
 *
 * Its server-side twin has had tests since the desktop build was found to be
 * subscribing to a bus nothing emitted on. This side had none at all, and it
 * carries the same two pieces of ordering care: subscribe before the hydration
 * fetch so no event is missed, and drop the fetch's answer if a live event
 * overtook it. Both survived deletion silently — the whole suite passed with
 * either removed.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const subscribeSseEvent = vi.fn();
const getProxyStatus = vi.fn();
const ingestProxyEvent = vi.fn();
const applyProxyStatus = vi.fn();
const resetProxyState = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ subscribe: subscribeSseEvent, getProxyStatus }),
}));

vi.mock('../../../src/services/proxyRegistry', () => ({
  ingestProxyEvent,
  applyProxyStatus,
  resetProxyState,
}));

async function loadFresh() {
  vi.resetModules();
  return import('../../../src/services/proxyEvents');
}

const PORT = 51733;

/** The daemon's status with nothing listening. */
const OFF = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null };

/** The daemon's status with the proxy up on `PORT`, as `GET /api/proxy/status` answers. */
const UP = {
  running: true,
  state: 'on',
  port: PORT,
  base_url: `http://127.0.0.1:${PORT}/v1`,
  key: 'none_needed',
  pinned_model: null,
};

describe('initProxyEvents', () => {
  beforeEach(() => {
    subscribeSseEvent.mockReset().mockReturnValue(() => {});
    getProxyStatus.mockReset().mockResolvedValue(OFF);
    ingestProxyEvent.mockReset();
    applyProxyStatus.mockReset();
    resetProxyState.mockReset();
  });

  afterEach(() => {
    vi.resetModules();
  });

  it('subscribes to the proxy category', async () => {
    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();

    expect(subscribeSseEvent).toHaveBeenCalledTimes(1);
    expect(subscribeSseEvent).toHaveBeenCalledWith('proxy', expect.any(Function));

    cleanupProxyEvents();
  });

  it('hydrates from the daemon\'s whole status, so a page load sees the port, the address and the key rule', async () => {
    getProxyStatus.mockResolvedValue(UP);

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    await vi.waitFor(() => expect(applyProxyStatus).toHaveBeenCalled());

    expect(applyProxyStatus).toHaveBeenCalledWith(UP);
    expect(ingestProxyEvent).not.toHaveBeenCalled();

    cleanupProxyEvents();
  });

  it('applies an off status too, so a registry that guessed up is corrected', async () => {
    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    await vi.waitFor(() => expect(applyProxyStatus).toHaveBeenCalled());

    expect(applyProxyStatus).toHaveBeenCalledWith(OFF);

    cleanupProxyEvents();
  });

  /**
   * An event carries the port and nothing else. The registry moves on it at
   * once, and the daemon is asked for the rest: the state word, where a
   * client reaches it, whether a request needs the key.
   */
  it('moves on an event at once, then reads the status again', async () => {
    let deliver: ((evt: unknown) => void) | undefined;
    subscribeSseEvent.mockImplementation((_c: string, cb: (e: unknown) => void) => {
      deliver = cb;
      return () => {};
    });

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    await vi.waitFor(() => expect(applyProxyStatus).toHaveBeenCalledTimes(1));

    getProxyStatus.mockResolvedValue(UP);
    deliver?.({ type: 'proxy_started', port: PORT });

    expect(ingestProxyEvent).toHaveBeenCalledWith({ type: 'proxy_started', port: PORT });
    await vi.waitFor(() => expect(applyProxyStatus).toHaveBeenCalledTimes(2));
    expect(applyProxyStatus).toHaveBeenLastCalledWith(UP);
    expect(getProxyStatus).toHaveBeenCalledTimes(2);

    cleanupProxyEvents();
  });

  /**
   * A live event during the hydration fetch is newer than the fetch. Applying
   * the status afterwards would resurrect a proxy that had just stopped.
   */
  it('drops hydration that a live event has already overtaken', async () => {
    let deliver: ((evt: unknown) => void) | undefined;
    subscribeSseEvent.mockImplementation((_c: string, cb: (e: unknown) => void) => {
      deliver = cb;
      return () => {};
    });

    let resolveStatus: (v: unknown) => void = () => {};
    // The hydration GET hangs until the event has landed; the event's own
    // re-read answers off, as a daemon that just stopped does.
    getProxyStatus
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveStatus = resolve;
        }),
      )
      .mockResolvedValue(OFF);

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();

    deliver?.({ type: 'proxy_stopped' });
    resolveStatus(UP);
    await vi.waitFor(() => expect(ingestProxyEvent).toHaveBeenCalled());
    // The event's own re-read is a second GET; the first one's answer is dropped.
    await vi.waitFor(() => expect(getProxyStatus).toHaveBeenCalledTimes(2));

    expect(ingestProxyEvent).toHaveBeenCalledTimes(1);
    expect(ingestProxyEvent).toHaveBeenCalledWith({ type: 'proxy_stopped' });
    expect(applyProxyStatus).not.toHaveBeenCalledWith(UP);
    expect(applyProxyStatus).toHaveBeenLastCalledWith(OFF);

    cleanupProxyEvents();
  });

  it('subscribes before the fetch resolves, so no event is missed', async () => {
    let resolveStatus: (v: unknown) => void = () => {};
    getProxyStatus.mockReturnValue(
      new Promise((resolve) => {
        resolveStatus = resolve;
      }),
    );

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();

    expect(subscribeSseEvent).toHaveBeenCalledTimes(1);

    resolveStatus({ running: false, port: null });
    cleanupProxyEvents();
  });

  it('only initializes once, however many callers ask', async () => {
    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    initProxyEvents();
    initProxyEvents();

    expect(subscribeSseEvent).toHaveBeenCalledTimes(1);

    cleanupProxyEvents();
  });

  it('unsubscribes on cleanup, and can be started again', async () => {
    const unsub = vi.fn();
    subscribeSseEvent.mockReturnValue(unsub);

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    cleanupProxyEvents();

    expect(unsub).toHaveBeenCalledTimes(1);
    expect(resetProxyState).toHaveBeenCalledTimes(1);

    initProxyEvents();
    expect(subscribeSseEvent).toHaveBeenCalledTimes(2);

    cleanupProxyEvents();
  });

  /**
   * Cleanup is newer than a fetch still out. Its status was asked for by a
   * bridge that is gone, and landing it would start a proxy in a registry
   * that cleanup had just emptied.
   */
  it('writes nothing from a status that answers after cleanup', async () => {
    let resolveStatus: (v: unknown) => void = () => {};
    getProxyStatus.mockReturnValue(
      new Promise((resolve) => {
        resolveStatus = resolve;
      }),
    );

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    expect(getProxyStatus).toHaveBeenCalledTimes(1);
    cleanupProxyEvents();

    resolveStatus(UP);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(applyProxyStatus).not.toHaveBeenCalled();
    expect(ingestProxyEvent).not.toHaveBeenCalled();
  });

  // `running` and `port` are separate fields that the daemon sets together.
  // A status with only one of them is not a running proxy with a port.
  it.each([
    ['running with no port', { running: true, port: null }],
    ['a port but not running', { running: false, port: PORT }],
  ])('says nothing for a status that is %s', async (_label, half) => {
    getProxyStatus.mockResolvedValue({
      ...half,
      base_url: null,
      key: null,
      pinned_model: null,
    });

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    expect(getProxyStatus).toHaveBeenCalledTimes(1);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(ingestProxyEvent).not.toHaveBeenCalled();

    cleanupProxyEvents();
  });
});
