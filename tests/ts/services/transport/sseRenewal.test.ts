/**
 * The shared stream after the desktop's service restarted: the new daemon
 * minted a new token, and the first attempt the probe's wake sets going
 * presents the old one. A 401 is the daemon answering, so once the token is
 * renewed the stream connects at once, not after a reconnect wait that grew
 * while the daemon was gone (up to 30 s). And a wake asked while the
 * refusal is still being handled (the renewal is out) is kept.
 *
 * The daemon is a fake `apiFetch`, with a token that goes stale; the
 * renewal is a fake that a test can hold.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const daemon = vi.hoisted(() => ({
  up: false,
  /** The stream presents a token this daemon did not mint. */
  staleToken: false,
  streams: [] as AbortSignal[],
  /** While set, a renewal waits on it, and fails when it rejects. */
  renewal: null as Promise<void> | null,
  renewals: 0,
  /** Whether a renewal gets the new token; a desktop app that still holds the old one does not. */
  renewHelps: true,
}));

vi.mock('../../../../src/services/transport/api/client', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport/api/client');
  const { TransportError } = await vi.importActual<typeof import('../../../../src/services/transport/errors')>(
    '../../../../src/services/transport/errors',
  );
  return {
    ...actual,
    apiFetch: async (_path: string, init: { signal?: AbortSignal } = {}) => {
      if (!daemon.up) throw new TypeError('fetch failed');
      if (daemon.staleToken) throw new TransportError('UNAUTHORIZED', 'refused', { status: 401 });
      const signal = init.signal as AbortSignal;
      daemon.streams.push(signal);
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          signal.addEventListener('abort', () => controller.error(new DOMException('aborted', 'AbortError')));
        },
      });
      return new Response(body, { status: 200 });
    },
  };
});
vi.mock('../../../../src/services/transport/api/renew', () => ({
  renewAfterRefusal: async (error: { details?: { status?: number } }) => {
    daemon.renewals += 1;
    if (daemon.renewal) await daemon.renewal;
    if (error?.details?.status !== 401) return false;
    if (daemon.renewHelps) daemon.staleToken = false;
    return true;
  },
}));

import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';

const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('the shared stream after a restart that minted a new token', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    Object.assign(daemon, { up: false, staleToken: false, streams: [], renewal: null, renewals: 0, renewHelps: true });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('opens at once after the wake, though its first attempt presents the old token', async () => {
    const manager = new SSEConnectionManager('/api/events');
    let opened = 0;
    manager.opened.listen(() => void (opened += 1));
    const stop = manager.subscribe(() => {});
    // 0.5 + 1 + 2 + 4 + 8 s failed: the loop now sleeps 16 s.
    await advance(15_600);
    daemon.up = true;
    daemon.staleToken = true;
    manager.retryNow(); // the probe heard the daemon

    await advance(10);
    expect(opened).toBe(1);
    await advance(40_000);
    expect(opened).toBe(1);
    expect(daemon.streams).toHaveLength(1);
    stop();
  });

  it('keeps a wake asked while a refusal is still being handled, and connects at once after it', async () => {
    const manager = new SSEConnectionManager('/api/events');
    let opened = 0;
    manager.opened.listen(() => void (opened += 1));
    const stop = manager.subscribe(() => {});
    await advance(7_600); // sleeping 8 s
    // The next attempt is refused, and handling that refusal takes a while
    // and fails (the credential could not be had).
    let fail: () => void = () => {};
    daemon.renewal = new Promise<void>((_resolve, reject) => {
      fail = () => reject(new Error('no token yet'));
    });
    const before = daemon.renewals;
    manager.retryNow();
    await advance(1);
    expect(daemon.renewals).toBe(before + 1);

    daemon.up = true;
    manager.retryNow(); // asked while the refusal is handled
    daemon.renewal = null;
    fail();
    await advance(10);

    expect(opened).toBe(1);
    stop();
  });

  it('after a renewal that did not help, waits out the backoff rather than ask again and again', async () => {
    const manager = new SSEConnectionManager('/api/events');
    daemon.up = true;
    // A token that stays stale: the renewal hands back the one already held.
    daemon.staleToken = true;
    daemon.renewHelps = false;
    const stop = manager.subscribe(() => {});
    await advance(100);
    // A refusal and its renewal, one attempt at once, refused again and
    // renewed again: then the wait, not a third attempt.
    expect(daemon.renewals).toBe(2);
    await advance(600);
    expect(daemon.renewals).toBe(3);
    expect(daemon.streams).toHaveLength(0);
    stop();
  });
});
