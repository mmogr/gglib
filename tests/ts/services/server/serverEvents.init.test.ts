/**
 * The bridge from the daemon's events into the server registry.
 *
 * These exist because the desktop build had this wrong and nothing noticed.
 * `initServerEvents` branched on `isDesktop()` and took a Tauri-event adapter
 * that listened for `server:*` names on the Tauri bus. Nothing emitted them —
 * the GUI backend had moved into the daemon and taken its `AppEventEmitter`
 * with it — so on desktop the registry was never populated and every consumer
 * of `useServerState` sat inert. The web build subscribed over SSE and was
 * fine, which is exactly why the bug survived.
 *
 * So the invariant under test is not "SSE works". It is **that there is only
 * one path**: whatever `isDesktop()` says, the registry gets subscribed and
 * hydrated the same way.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const subscribe = vi.fn();
const listServers = vi.fn();
const ingestServerEvent = vi.fn();
const isDesktop = vi.fn();

vi.mock('../../../../src/services/transport', () => ({
  getTransport: () => ({ subscribe, listServers }),
}));

vi.mock('../../../../src/services/serverRegistry', () => ({
  ingestServerEvent,
}));

vi.mock('../../../../src/services/platform', () => ({
  isDesktop,
}));

async function loadFresh() {
  vi.resetModules();
  return import('../../../../src/services/serverEvents');
}

describe('initServerEvents', () => {
  beforeEach(() => {
    subscribe.mockReset().mockReturnValue(() => {});
    listServers.mockReset().mockResolvedValue([]);
    ingestServerEvent.mockReset();
    isDesktop.mockReset();
  });

  afterEach(() => {
    vi.resetModules();
  });

  // The regression itself. Parameterised over the platform because a fix that
  // only asserts one of them would have passed against the broken code.
  it.each([
    ['desktop', true],
    ['web', false],
  ])('subscribes through the transport on %s', async (_label, desktop) => {
    isDesktop.mockReturnValue(desktop);

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();

    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(subscribe).toHaveBeenCalledWith('server', expect.any(Function));

    cleanupServerEvents();
  });

  it('hydrates from the server list so a page load sees what is already running', async () => {
    listServers.mockResolvedValue([{ modelId: 1, port: 8080 }]);

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();
    await vi.waitFor(() => expect(ingestServerEvent).toHaveBeenCalled());

    expect(ingestServerEvent).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'snapshot' }),
    );

    cleanupServerEvents();
  });

  /**
   * A live event during the hydration fetch is newer than the fetch. Applying
   * the snapshot afterwards would resurrect a server that had just stopped.
   */
  it('drops hydration that a live event has already overtaken', async () => {
    let deliver: ((payload: unknown) => void) | undefined;
    subscribe.mockImplementation((_channel: string, cb: (p: unknown) => void) => {
      deliver = cb;
      return () => {};
    });

    let resolveList: (v: unknown[]) => void = () => {};
    listServers.mockReturnValue(
      new Promise<unknown[]>((resolve) => {
        resolveList = resolve;
      }),
    );

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();

    deliver?.({ type: 'server_stopped', modelId: 1, port: 8080 });
    resolveList([{ modelId: 1, port: 8080 }]);
    await vi.waitFor(() => expect(ingestServerEvent).toHaveBeenCalled());

    expect(ingestServerEvent).toHaveBeenCalledTimes(1);
    expect(ingestServerEvent).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'stopped' }),
    );

    cleanupServerEvents();
  });

  it('only initializes once, however many callers ask', async () => {
    const { initServerEvents, cleanupServerEvents } = await loadFresh();

    initServerEvents();
    initServerEvents();

    expect(subscribe).toHaveBeenCalledTimes(1);

    cleanupServerEvents();
  });

  it('unsubscribes on cleanup, and can be started again', async () => {
    const unsubscribe = vi.fn();
    subscribe.mockReturnValue(unsubscribe);

    const { initServerEvents, cleanupServerEvents } = await loadFresh();

    initServerEvents();
    cleanupServerEvents();
    expect(unsubscribe).toHaveBeenCalledTimes(1);

    initServerEvents();
    expect(subscribe).toHaveBeenCalledTimes(2);

    cleanupServerEvents();
  });
});
