/**
 * The shared stream's reconnect loop: a loop stopped while it sleeps in a
 * reconnect wait ends when it wakes, however soon a new reader starts the
 * stream again, so there is one stream (TX-1); and a reader that knows the
 * daemon is back can cut the wait short (`retryNow`), even one it asks for
 * while an attempt is still out; asked while connected, the opening is
 * announced again on the one stream.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

/** The fake `/api/events`: up or not, and every stream it handed out. */
const daemon = vi.hoisted(() => ({
  up: false,
  streams: [] as AbortSignal[],
  /** While set, an attempt waits on it, and fails when it rejects. */
  held: null as Promise<void> | null,
  /** Ends each open stream, as a daemon going does. */
  ends: [] as (() => void)[],
}));

vi.mock('../../../../src/services/transport/api/client', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport/api/client');
  return {
    ...actual,
    apiFetch: async (_path: string, init: { signal?: AbortSignal } = {}) => {
      if (daemon.held) await daemon.held;
      if (!daemon.up) throw new Error('fetch failed');
      const signal = init.signal as AbortSignal;
      daemon.streams.push(signal);
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          signal.addEventListener('abort', () => controller.error(new DOMException('aborted', 'AbortError')));
          daemon.ends.push(() => controller.error(new TypeError('network error')));
        },
      });
      return new Response(body, { status: 200 });
    },
  };
});

import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';

const open = () => daemon.streams.filter((signal) => !signal.aborted).length;
const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('the shared stream reconnecting', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    Object.assign(daemon, { up: false, streams: [], held: null, ends: [] });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('a loop stopped in its reconnect wait does not carry on beside the next one', async () => {
    const manager = new SSEConnectionManager('/api/events');
    const stopFirst = manager.subscribe(() => {});
    // Failing, so its waits grow past a second: it is asleep now.
    await advance(2000);
    stopFirst();
    daemon.up = true;
    const stopSecond = manager.subscribe(() => {});
    await advance(60_000);
    expect(open()).toBe(1);
    stopSecond();
    expect(open()).toBe(0);
  });

  it('retryNow connects at once instead of sleeping out the wait', async () => {
    const manager = new SSEConnectionManager('/api/events');
    const stop = manager.subscribe(() => {});
    // 0.5 + 1 + 2 + 4 s failed: the loop now sleeps 8 s.
    await advance(7600);
    daemon.up = true;
    await advance(100);
    expect(open()).toBe(0);
    manager.retryNow();
    await advance(1);
    expect(open()).toBe(1);
    stop();
  });

  // The daemon is heard while an attempt begun before it listened is still
  // out: that attempt fails, and the next is made at once, not after a wait
  // that grew while the daemon was gone.
  it('keeps a retryNow asked while an attempt is out, and connects at once when that attempt fails', async () => {
    const manager = new SSEConnectionManager('/api/events');
    const stop = manager.subscribe(() => {});
    // 0.5 + 1 + 2 + 4 s failed: the loop now sleeps 8 s.
    await advance(7600);
    let refuse: () => void = () => {};
    daemon.held = new Promise<void>((_resolve, reject) => {
      refuse = () => reject(new Error('fetch failed'));
    });
    manager.retryNow();
    await advance(1);

    daemon.up = true;
    manager.retryNow();
    daemon.held = null;
    refuse();
    await advance(1);

    expect(open()).toBe(1);
    stop();
  });

  // The health probe failed for a poll while the stream stayed up, and the
  // readers let their state go: told the daemon is back, they must read it
  // again, and the stream that never closed is kept.
  it('retryNow while connected keeps the one stream and announces its opening again', async () => {
    daemon.up = true;
    const manager = new SSEConnectionManager('/api/events');
    let opened = 0;
    manager.opened.listen(() => void (opened += 1));
    const stop = manager.subscribe(() => {});
    await advance(1);
    expect(opened).toBe(1);

    manager.retryNow();
    await advance(1);

    expect(opened).toBe(2);
    expect(daemon.streams).toHaveLength(1);
    expect(open()).toBe(1);
    stop();
  });

  it('retryNow does nothing once stopped', async () => {
    daemon.up = true;
    const manager = new SSEConnectionManager('/api/events');
    let opened = 0;
    manager.opened.listen(() => void (opened += 1));
    const stop = manager.subscribe(() => {});
    await advance(1);
    stop();

    manager.retryNow();
    await advance(1);

    expect(opened).toBe(1);
    expect(daemon.streams).toHaveLength(1);
    expect(open()).toBe(0);
  });

  // A store asks a failed read again only while the stream is open, so it
  // must hear when an open stream ends, however it ends.
  it('says whether it is open, and announces each end of an open stream: failed, and stopped', async () => {
    daemon.up = true;
    const manager = new SSEConnectionManager('/api/events');
    let closed = 0;
    manager.closed.listen(() => void (closed += 1));
    expect(manager.isOpen).toBe(false);
    const stop = manager.subscribe(() => {});
    await advance(1);
    expect(manager.isOpen).toBe(true);
    expect(closed).toBe(0);

    daemon.up = false;
    for (const end of daemon.ends.splice(0)) end();
    await advance(1);
    expect(manager.isOpen).toBe(false);
    expect(closed).toBe(1);

    daemon.up = true;
    await advance(1000);
    expect(manager.isOpen).toBe(true);
    stop();
    expect(manager.isOpen).toBe(false);
    expect(closed).toBe(2);
    await advance(1000);
    expect(closed).toBe(2);
  });
});
