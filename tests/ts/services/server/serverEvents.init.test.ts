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

  /**
   * The fixture is the shape `GET /api/servers` really returns — snake_case,
   * with a `pid`. It used to be camelCase, and the assertion used to stop at
   * `{ type: 'snapshot' }`, so between them the test passed on an empty
   * snapshot: it could not fail for the reason it exists.
   */
  it('hydrates from the server list so a page load sees what is already running', async () => {
    listServers.mockResolvedValue([
      { model_id: 1, model_name: 'Loaded', pid: 4242, port: 8080, started_at: 1_700_000_000 },
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
  it('drops hydration that a live event has already overtaken', async () => {
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

    // `server_stopped` carries no port — the Rust variant has `modelId` and
    // `modelName` only.
    deliver?.({ type: 'server_stopped', modelId: 1, modelName: 'Loaded' });
    resolveList([
      { model_id: 1, model_name: 'Loaded', pid: null, port: 8080, started_at: 1_700_000_000 },
    ]);
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
