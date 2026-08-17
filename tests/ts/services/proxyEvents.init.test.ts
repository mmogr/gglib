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
const resetProxyState = vi.fn();

vi.mock('../../../src/services/transport/events/sse', () => ({ subscribeSseEvent }));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ getProxyStatus }),
}));

vi.mock('../../../src/services/proxyRegistry', () => ({
  ingestProxyEvent,
  resetProxyState,
}));

async function loadFresh() {
  vi.resetModules();
  return import('../../../src/services/proxyEvents');
}

const PORT = 51733;

describe('initProxyEvents', () => {
  beforeEach(() => {
    subscribeSseEvent.mockReset().mockReturnValue(() => {});
    getProxyStatus.mockReset().mockResolvedValue({
      running: false,
      port: null,
      current_model: null,
      model_port: null,
      pinned_model: null,
    });
    ingestProxyEvent.mockReset();
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

  it('hydrates from a running proxy so a page load sees the port', async () => {
    getProxyStatus.mockResolvedValue({
      running: true,
      port: PORT,
      current_model: null,
      model_port: null,
      pinned_model: null,
    });

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    await vi.waitFor(() => expect(ingestProxyEvent).toHaveBeenCalled());

    expect(ingestProxyEvent).toHaveBeenCalledWith({ type: 'proxy_started', port: PORT });

    cleanupProxyEvents();
  });

  it('says nothing when the proxy is not running', async () => {
    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();
    await vi.waitFor(() => expect(getProxyStatus).toHaveBeenCalled());

    expect(ingestProxyEvent).not.toHaveBeenCalled();

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
    getProxyStatus.mockReturnValue(
      new Promise((resolve) => {
        resolveStatus = resolve;
      }),
    );

    const { initProxyEvents, cleanupProxyEvents } = await loadFresh();
    initProxyEvents();

    deliver?.({ type: 'proxy_stopped' });
    resolveStatus({
      running: true,
      port: PORT,
      current_model: null,
      model_port: null,
      pinned_model: null,
    });
    await vi.waitFor(() => expect(ingestProxyEvent).toHaveBeenCalled());

    expect(ingestProxyEvent).toHaveBeenCalledTimes(1);
    expect(ingestProxyEvent).toHaveBeenCalledWith({ type: 'proxy_stopped' });

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
});
