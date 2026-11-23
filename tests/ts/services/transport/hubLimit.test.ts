/**
 * `whenHubAllows`: a request the Hub's limit refused waits until a second
 * after the limit lifts and is made again, telling `onWaiting` when the wait
 * starts and ends; a stop during the wait ends it and asks nothing more;
 * the fifth limited answer in a row is thrown; any other error at once.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { TransportError } from '../../../../src/services/transport/errors';
import { whenHubAllows } from '../../../../src/services/transport/hubLimit';

const limited = (secs: number) =>
  new TransportError('INTERNAL', `Failed to get quantizations: Hugging Face's rate limit lifts in 0:${String(secs).padStart(2, '0')}`, {
    status: 429,
    statusText: 'Too Many Requests',
    type: 'hub_rate_limited',
    metadata: { bucket: 'api', retry_after_secs: secs },
  });

describe('whenHubAllows', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 10, 24, 9, 0, 0));
  });
  afterEach(() => vi.useRealTimers());

  it('waits until a second after the limit lifts, then asks again and answers', async () => {
    const action = vi.fn().mockRejectedValueOnce(limited(10)).mockResolvedValueOnce('listing');
    const onWaiting = vi.fn();
    const start = Date.now();

    const answer = whenHubAllows(action, { onWaiting });
    await vi.advanceTimersByTimeAsync(0);
    expect(onWaiting).toHaveBeenCalledWith(start + 11_000);
    expect(action).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(10_999);
    expect(action).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);

    await expect(answer).resolves.toBe('listing');
    expect(action).toHaveBeenCalledTimes(2);
    expect(onWaiting.mock.calls).toEqual([[start + 11_000], [null]]);
  });

  it('ends the wait at once when stopped, and asks nothing more', async () => {
    const action = vi.fn().mockRejectedValue(limited(30));
    const stop = new AbortController();
    const answer = whenHubAllows(action, { signal: stop.signal });
    const settled = expect(answer).rejects.toMatchObject({ name: 'AbortError' });
    await vi.advanceTimersByTimeAsync(5_000);

    stop.abort();
    await settled;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(action).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('throws the fifth limited answer in a row', async () => {
    const last = limited(1);
    const action = vi.fn().mockRejectedValueOnce(limited(1)).mockRejectedValueOnce(limited(1)).mockRejectedValueOnce(limited(1)).mockRejectedValueOnce(limited(1)).mockRejectedValueOnce(last);
    const onWaiting = vi.fn();
    const answer = whenHubAllows(action, { onWaiting });
    const settled = expect(answer).rejects.toBe(last);

    await vi.advanceTimersByTimeAsync(60_000);
    await settled;
    expect(action).toHaveBeenCalledTimes(5);
    expect(onWaiting.mock.calls.filter(([at]) => at !== null)).toHaveLength(4);
  });

  it('throws any other error as it came, without waiting', async () => {
    const busy = new TransportError('INTERNAL', 'busy', { status: 429, statusText: 'Too Many Requests', type: 'agent_busy' });
    const action = vi.fn().mockRejectedValue(busy);
    const onWaiting = vi.fn();

    await expect(whenHubAllows(action, { onWaiting })).rejects.toBe(busy);
    expect(action).toHaveBeenCalledTimes(1);
    expect(onWaiting).not.toHaveBeenCalled();
  });
});
