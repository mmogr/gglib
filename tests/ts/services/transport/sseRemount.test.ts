/**
 * The shared stream stopped and started again in one tick, as a StrictMode
 * remount does (`main.tsx`): the stopped loop's attempt is still settling
 * when the new loop's is out. What the old loop does as it settles is its
 * own, so a wake asked during the new loop's attempt is kept for that
 * attempt, and the next is made at once when it is refused.
 *
 * The daemon is a fake `apiFetch` whose attempts hang until refused.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const daemon = vi.hoisted(() => ({ pending: [] as ((error: Error) => void)[] }));

vi.mock('../../../../src/services/transport/api/client', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport/api/client');
  return {
    ...actual,
    apiFetch: (_path: string, init: { signal?: AbortSignal } = {}) =>
      new Promise<Response>((_resolve, reject) => {
        const signal = init.signal as AbortSignal;
        signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
        // An attempt that hangs until refused (a daemon not listening yet, slow to refuse).
        daemon.pending.push(reject);
      }),
  };
});

import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';

describe('the shared stream stopped and started in one tick', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    daemon.pending = [];
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("keeps a wake asked during the new loop's first attempt", async () => {
    const manager = new SSEConnectionManager('/api/events');
    const stopFirst = manager.subscribe(() => {});
    stopFirst();
    const stopSecond = manager.subscribe(() => {});
    await vi.advanceTimersByTimeAsync(1); // the old loop's aborted attempt settles
    manager.retryNow(); // asked while the new loop's attempt is out
    expect(daemon.pending).toHaveLength(2);

    daemon.pending[1](new TypeError('refused'));
    await vi.advanceTimersByTimeAsync(1);

    expect(daemon.pending).toHaveLength(3); // the next attempt, made at once
    stopSecond();
  });
});
