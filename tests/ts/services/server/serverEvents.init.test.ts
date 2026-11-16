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

import type { ServerWireEvent } from '../../../../src/services/transport/types/events';
import type { ServerInfo } from '../../../../src/types';

const subscribe = vi.fn();
const listServers = vi.fn();
const ingestServerEvent = vi.fn();
const isDesktop = vi.fn();
/** Who listens for the event stream to open or close; a test opens it by calling them. */
const openListeners = new Set<() => void>();
const closeListeners = new Set<() => void>();
const stream = { open: false };
const onEventStreamOpen = (listener: () => void) => {
  openListeners.add(listener);
  return () => void openListeners.delete(listener);
};
const onEventStreamClose = (listener: () => void) => {
  closeListeners.add(listener);
  return () => void closeListeners.delete(listener);
};
const isEventStreamOpen = () => stream.open;
const openStream = () => {
  stream.open = true;
  openListeners.forEach((listener) => listener());
};

vi.mock('../../../../src/services/transport', () => ({
  getTransport: () => ({ subscribe, onEventStreamOpen, onEventStreamClose, isEventStreamOpen, listServers }),
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
    openListeners.clear();
    closeListeners.clear();
    stream.open = false;
  });

  afterEach(() => {
    vi.useRealTimers();
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

  /**
   * The fixture is the shape `GET /api/servers` really returns — snake_case,
   * with a `pid`. It used to be camelCase, and the assertion used to stop at
   * `{ type: 'snapshot' }`, so between them the test passed on an empty
   * snapshot: it could not fail for the reason it exists.
   */
  it('hydrates from the server list so a page load sees what is already running', async () => {
    listServers.mockResolvedValue([
      {
        model_id: 1,
        model_name: 'Loaded',
        pid: 4242,
        port: 8080,
        started_at: 1_700_000_000,
        runtime: 'llama',
      },
    ]);

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();
    await vi.waitFor(() => expect(ingestServerEvent).toHaveBeenCalled());

    expect(ingestServerEvent).toHaveBeenCalledWith({
      type: 'snapshot',
      servers: [
        {
          modelId: '1',
          modelName: 'Loaded',
          status: 'running',
          port: 8080,
          updatedAt: 1_700_000_000_000,
        },
      ],
    });

    cleanupServerEvents();
  });

  /**
   * A live event during the hydration fetch is newer than the fetch. Applying
   * the snapshot afterwards would resurrect a server that had just stopped.
   */
  it('drops hydration that a live event has already overtaken, and lands the read the event set going', async () => {
    // Typed on both sides deliberately. `ServerWireEvent` and `ServerInfo` are
    // otherwise read by nothing in a checked position — the subscription hands
    // its payload to a normalizer taking `unknown` — so corrupting either
    // passed typecheck and the whole suite. These two fixtures are what make
    // them load-bearing.
    let deliver: ((payload: ServerWireEvent) => void) | undefined;
    subscribe.mockImplementation((_channel: string, cb: (p: ServerWireEvent) => void) => {
      deliver = cb;
      return () => {};
    });

    let resolveList: (v: ServerInfo[]) => void = () => {};
    listServers.mockReturnValue(
      new Promise<ServerInfo[]>((resolve) => {
        resolveList = resolve;
      }),
    );

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();

    // The read the event sets going answers with nothing running.
    listServers.mockResolvedValue([]);
    // `server_stopped` carries no port — the Rust variant has `modelId` and
    // `modelName` only.
    deliver?.({ type: 'server_stopped', modelId: 1, modelName: 'Loaded' });
    resolveList([
      {
        model_id: 1,
        model_name: 'Loaded',
        pid: null,
        port: 8080,
        started_at: 1_700_000_000,
        runtime: 'llama',
      },
    ]);
    await vi.waitFor(() => expect(ingestServerEvent).toHaveBeenCalledTimes(2));

    // The overtaken hydration is dropped; the read the event set going (the
    // list still unheard) is the one that lands.
    expect(listServers).toHaveBeenCalledTimes(2);
    expect(ingestServerEvent.mock.calls.map(([evt]) => evt)).toEqual([
      expect.objectContaining({ type: 'stopped' }),
      { type: 'snapshot', servers: [] },
    ]);

    cleanupServerEvents();
  });

  // While the stream is down nothing asks on a timer: a read that failed is
  // followed by the next opening of the stream (the daemon answering) or the
  // next server event, which says so too.
  it('while the stream is down, reads a list that failed again on a server event or an opening of the stream, not on a timer', async () => {
    vi.useFakeTimers();
    let deliver: ((payload: ServerWireEvent) => void) | undefined;
    subscribe.mockImplementation((_channel: string, cb: (p: ServerWireEvent) => void) => {
      deliver = cb;
      return () => {};
    });
    listServers.mockRejectedValueOnce(new Error('refused')).mockRejectedValueOnce(new Error('refused'));
    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();

    await vi.advanceTimersByTimeAsync(30_000);
    expect(listServers).toHaveBeenCalledTimes(1);

    deliver?.({ type: 'server_stopped', modelId: 1, modelName: 'Loaded' });
    await vi.advanceTimersByTimeAsync(0);
    expect(listServers).toHaveBeenCalledTimes(2);

    openStream();
    await vi.advanceTimersByTimeAsync(0);
    expect(listServers).toHaveBeenCalledTimes(3);

    // Heard: an event no longer reads the list.
    deliver?.({ type: 'server_stopped', modelId: 1, modelName: 'Loaded' });
    await vi.advanceTimersByTimeAsync(30_000);
    expect(listServers).toHaveBeenCalledTimes(3);

    cleanupServerEvents();
  });

  // The review's sequence: the mount read and the opening's read both fail
  // (a 30 s time limit, a 5xx), the stream stays open, no server starts. The
  // one retry, `keptCurrent`'s, asks again after 1 s, then 2 s, until a read lands.
  it('while the stream is open, asks a list that failed again after a wait that grows, until one lands', async () => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    const timedOut = () => Promise.reject(new Error('gglib did not answer in 30 seconds.'));
    listServers.mockImplementationOnce(timedOut).mockImplementationOnce(timedOut).mockImplementationOnce(timedOut);
    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();
    openStream();
    await vi.advanceTimersByTimeAsync(0);
    expect(listServers).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(999);
    expect(listServers).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(listServers).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(2000);
    expect(listServers).toHaveBeenCalledTimes(4);
    expect(ingestServerEvent).toHaveBeenCalledWith({ type: 'snapshot', servers: [] });

    // Landed: nothing more.
    await vi.advanceTimersByTimeAsync(10 * 60_000);
    expect(listServers).toHaveBeenCalledTimes(4);
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

  /**
   * Cleanup is newer than a fetch still out. Its list was asked for by a
   * bridge that is gone, and must not land in the registry after it.
   */
  it('writes nothing from a list that answers after cleanup', async () => {
    let resolveList: (v: ServerInfo[]) => void = () => {};
    listServers.mockReturnValue(
      new Promise<ServerInfo[]>((resolve) => {
        resolveList = resolve;
      }),
    );

    const { initServerEvents, cleanupServerEvents } = await loadFresh();
    initServerEvents();
    expect(listServers).toHaveBeenCalledTimes(1);
    cleanupServerEvents();

    resolveList([
      {
        model_id: 1,
        model_name: 'Loaded',
        pid: 4242,
        port: 8080,
        started_at: 1_700_000_000,
        runtime: 'llama',
      },
    ]);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(ingestServerEvent).not.toHaveBeenCalled();
  });
});
