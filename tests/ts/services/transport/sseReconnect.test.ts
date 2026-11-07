/**
 * The shared stream's reconnect loop: a loop stopped while it sleeps in a
 * reconnect wait ends when it wakes, however soon a new reader starts the
 * stream again, so there is one stream (TX-1); and a reader that knows the
 * daemon is back can cut the wait short (`retryNow`).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

/** The fake `/api/events`: up or not, and every stream it handed out. */
const daemon = vi.hoisted(() => ({ up: false, streams: [] as AbortSignal[] }));

vi.mock('../../../../src/services/transport/api/client', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport/api/client');
  return {
    ...actual,
    apiFetch: async (_path: string, init: { signal?: AbortSignal } = {}) => {
      if (!daemon.up) throw new Error('fetch failed');
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

import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';

const open = () => daemon.streams.filter((signal) => !signal.aborted).length;
const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('the shared stream reconnecting', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    Object.assign(daemon, { up: false, streams: [] });
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

  it('retryNow does nothing while connected', async () => {
    daemon.up = true;
    const manager = new SSEConnectionManager('/api/events');
    const stop = manager.subscribe(() => {});
    await advance(1);
    manager.retryNow();
    await advance(1);
    expect(daemon.streams).toHaveLength(1);
    expect(open()).toBe(1);
    stop();
  });
});
