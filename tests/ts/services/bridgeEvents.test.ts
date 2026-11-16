/**
 * The one bridge from a category of daemon events into a registry, and the
 * reads that keep a store current (`keptCurrent`).
 *
 * The server, proxy, remote, model and download registries are all fed
 * through the bridge, and what it owns is the order: subscribe before the
 * read, and apply a read only if nothing happened while it was out — no
 * event, no cleanup, and no later read already landed. Each registry has its
 * own test of that through its own bridge; these hold the sequence itself,
 * where it is written.
 *
 * `keptCurrent` is what every store that reads the daemon is kept current
 * by, the bridges among them: read now, and again each time the event
 * stream opens, the first time and every reconnection. The stream opening is
 * the signal that the daemon answers, so a first read the daemon refused
 * (not yet listening, on a cold start) is followed by one that lands.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import type { ProxyEvent } from '../../../src/services/transport/types/events';

const subscribe = vi.fn();
/** Who is listening for the stream to open. */
const openListeners = new Set<() => void>();
const onEventStreamOpen = vi.fn((listener: () => void) => {
  openListeners.add(listener);
  return () => {
    openListeners.delete(listener);
  };
});
/** Who is listening for an open stream to end, and whether one is open. */
const closeListeners = new Set<() => void>();
const stream = { open: false };
const onEventStreamClose = (listener: () => void) => {
  closeListeners.add(listener);
  return () => void closeListeners.delete(listener);
};
const isEventStreamOpen = () => stream.open;
/** The stream opens, as on its first connection or on a reconnection. */
const openStream = () => {
  stream.open = true;
  [...openListeners].forEach((listener) => listener());
};
/** An open stream ends. */
const closeStream = () => {
  stream.open = false;
  [...closeListeners].forEach((listener) => listener());
};

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ subscribe, onEventStreamOpen, onEventStreamClose, isEventStreamOpen }),
}));

import { bridgeEvents, keptCurrent } from '../../../src/services/bridgeEvents';

const STOPPED: ProxyEvent = { type: 'proxy_stopped' };

/** A read the test answers when it chooses. */
function pendingRead() {
  let answer: (value: string) => void = () => {};
  const promise = new Promise<string>((resolve) => {
    answer = resolve;
  });
  return { promise, answer };
}

/** Let every answered read run to its end. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('bridgeEvents', () => {
  const unsubscribe = vi.fn();
  const onEvent = vi.fn();
  const read = vi.fn<() => Promise<string>>();
  const apply = vi.fn();
  const reset = vi.fn();
  /** Send one event the way the stream would, to the latest subscription. */
  let deliver: (evt: ProxyEvent) => void = () => {};

  const bridge = () => bridgeEvents({ category: 'proxy', onEvent, read, apply, reset });

  beforeEach(() => {
    openListeners.clear();
    closeListeners.clear();
    stream.open = false;
    unsubscribe.mockReset();
    onEvent.mockReset();
    read.mockReset().mockResolvedValue('now');
    apply.mockReset();
    reset.mockReset();
    subscribe.mockReset().mockImplementation((_category: string, listener: (evt: ProxyEvent) => void) => {
      deliver = listener;
      return unsubscribe;
    });
  });

  it('subscribes to its category before it asks for the read, then applies the answer', async () => {
    const order: string[] = [];
    subscribe.mockImplementation(() => {
      order.push('subscribe');
      return unsubscribe;
    });
    read.mockImplementation(() => {
      order.push('read');
      return Promise.resolve('now');
    });

    bridge().init();

    expect(order).toEqual(['subscribe', 'read']);
    expect(subscribe).toHaveBeenCalledWith('proxy', expect.any(Function));
    await settle();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith('now');
  });

  it('hands each event to the registry', () => {
    bridge().init();
    deliver(STOPPED);

    expect(onEvent).toHaveBeenCalledTimes(1);
    expect(onEvent).toHaveBeenCalledWith(STOPPED);
  });

  it('drops a read that an event overtook', async () => {
    const hydration = pendingRead();
    read.mockReturnValueOnce(hydration.promise);
    bridge().init();

    deliver(STOPPED);
    hydration.answer('older than the event');
    await settle();

    expect(onEvent).toHaveBeenCalledWith(STOPPED);
    expect(apply).not.toHaveBeenCalled();
  });

  // The count a read is held to is the one it began at, not zero: a bridge
  // that has seen events can still be read into.
  it('applies a read that began after the last event', async () => {
    const b = bridge();
    b.init();
    await settle();
    deliver(STOPPED);
    apply.mockReset();

    await expect(b.refresh()).resolves.toBe('applied');
    expect(apply).toHaveBeenCalledWith('now');
  });

  // The count moves before the event is handed on, so a read the handler
  // starts is of the state after the event, and lands.
  it('applies a read that the event\'s own handler started', async () => {
    const b = bridge();
    b.init();
    await settle();
    apply.mockReset();

    read.mockResolvedValueOnce('after the event');
    onEvent.mockImplementationOnce(() => {
      void b.refresh();
    });
    deliver(STOPPED);
    await settle();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith('after the event');
  });

  it('drops a read that answers after cleanup', async () => {
    const hydration = pendingRead();
    read.mockReturnValueOnce(hydration.promise);
    const b = bridge();
    b.init();
    b.cleanup();

    hydration.answer('asked for by a bridge that is gone');
    await settle();

    expect(apply).not.toHaveBeenCalled();
  });

  // Starting again does not put the count back: a read from before the
  // cleanup must not pass for one of the new subscription's.
  it('still drops a read from before a cleanup once it has been started again', async () => {
    const before = pendingRead();
    const after = pendingRead();
    read.mockReturnValueOnce(before.promise).mockReturnValueOnce(after.promise);
    const b = bridge();
    b.init();
    b.cleanup();
    b.init();

    before.answer('from before the cleanup');
    await settle();
    expect(apply).not.toHaveBeenCalled();

    after.answer('from the new subscription');
    await settle();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith('from the new subscription');
  });

  it('starts once however often it is asked, and again after cleanup', () => {
    const b = bridge();
    b.init();
    b.init();
    b.init();
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(read).toHaveBeenCalledTimes(1);

    b.cleanup();
    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(reset).toHaveBeenCalledTimes(1);

    b.init();
    expect(subscribe).toHaveBeenCalledTimes(2);
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('empties the registry on cleanup even when it was never started, and needs no reset to be given', () => {
    bridge().cleanup();
    expect(reset).toHaveBeenCalledTimes(1);
    expect(unsubscribe).not.toHaveBeenCalled();

    const withoutReset = bridgeEvents({ category: 'proxy', onEvent, read, apply });
    withoutReset.init();
    expect(() => withoutReset.cleanup()).not.toThrow();
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it('says whether a read was applied or failed, and never rejects', async () => {
    const b = bridge();
    await expect(b.refresh()).resolves.toBe('applied');
    expect(apply).toHaveBeenCalledWith('now');
    apply.mockReset();

    read.mockRejectedValueOnce(new Error('daemon gone'));
    await expect(b.refresh()).resolves.toBe('failed');

    read.mockImplementationOnce(() => {
      throw new Error('no transport');
    });
    await expect(b.refresh()).resolves.toBe('failed');
    expect(apply).not.toHaveBeenCalled();

    apply.mockImplementationOnce(() => {
      throw new Error('registry refused it');
    });
    await expect(b.refresh()).resolves.toBe('failed');
  });

  it('says a read an event overtook was superseded, which is not a failure', async () => {
    const b = bridge();
    b.init();
    await settle();
    apply.mockReset();

    const overtaken = pendingRead();
    read.mockReturnValueOnce(overtaken.promise);
    const reread = b.refresh();
    deliver(STOPPED);
    overtaken.answer('older than the event');

    await expect(reread).resolves.toBe('superseded');
    expect(apply).not.toHaveBeenCalled();
  });
  it('reads again each time the stream opens, the first time and every reconnection', async () => {
    bridge().init();
    await settle();
    expect(read).toHaveBeenCalledTimes(1);

    read.mockResolvedValueOnce('after the first open');
    openStream();
    await settle();
    read.mockResolvedValueOnce('after a reconnection');
    openStream();
    await settle();

    expect(read).toHaveBeenCalledTimes(3);
    expect(apply.mock.calls).toEqual([['now'], ['after the first open'], ['after a reconnection']]);
  });

  // A3: on a cold start the first read is refused, the daemon not yet
  // listening; the stream opening says it is, and the read then lands.
  it('follows a first read the daemon refused with one that lands when the stream opens', async () => {
    read.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    bridge().init();
    await settle();
    expect(apply).not.toHaveBeenCalled();

    openStream();
    await settle();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith('now');
  });

  it('never lets a slower earlier read overwrite a later one', async () => {
    const slow = pendingRead();
    read.mockReturnValueOnce(slow.promise).mockResolvedValueOnce('later');
    bridge().init();
    openStream();
    await settle();
    expect(apply).toHaveBeenCalledWith('later');

    slow.answer('earlier');
    await settle();

    expect(apply).toHaveBeenCalledTimes(1);
  });

  it('says a read that a later read overtook was superseded', async () => {
    const b = bridge();
    const slow = pendingRead();
    read.mockReturnValueOnce(slow.promise).mockResolvedValueOnce('later');
    const first = b.refresh();
    await expect(b.refresh()).resolves.toBe('applied');
    slow.answer('earlier');

    await expect(first).resolves.toBe('superseded');
    expect(apply.mock.calls).toEqual([['later']]);
  });

  it('runs its onStreamOpen before the read an opening sets going', async () => {
    const order: string[] = [];
    const b = bridgeEvents({ category: 'proxy', onEvent, read, apply, onStreamOpen: () => order.push('opened') });
    b.init();
    await settle();
    read.mockImplementation(() => {
      order.push('read');
      return Promise.resolve('now');
    });

    openStream();

    expect(order).toEqual(['opened', 'read']);
  });

  it('reads nothing on an opening after cleanup', async () => {
    const b = bridge();
    b.init();
    await settle();
    b.cleanup();

    openStream();
    await settle();

    expect(read).toHaveBeenCalledTimes(1);
  });
});

describe('keptCurrent', () => {
  const read = vi.fn<() => Promise<string>>();
  const apply = vi.fn();

  const kept = () => keptCurrent({ read, apply });

  beforeEach(() => {
    openListeners.clear();
    closeListeners.clear();
    stream.open = false;
    read.mockReset().mockResolvedValue('now');
    apply.mockReset();
  });

  it('reads now, and applies the answer', async () => {
    kept().keep();
    expect(read).toHaveBeenCalledTimes(1);
    await settle();
    expect(apply).toHaveBeenCalledWith('now');
  });

  it('reads again each time the stream opens, the first time and every reconnection', async () => {
    kept().keep();
    await settle();

    openStream();
    await settle();
    openStream();
    await settle();

    expect(read).toHaveBeenCalledTimes(3);
    expect(apply).toHaveBeenCalledTimes(3);
  });

  it('follows a first read that failed with one that is applied once the stream opens', async () => {
    read.mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce('the daemon answers');
    kept().keep();
    await settle();
    expect(apply).not.toHaveBeenCalled();

    openStream();
    await settle();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith('the daemon answers');
  });

  it('never lets a slower earlier read overwrite a later one', async () => {
    const slow = pendingRead();
    read.mockReturnValueOnce(slow.promise).mockResolvedValueOnce('later');
    const k = kept();
    k.keep();
    openStream();
    await settle();
    expect(apply.mock.calls).toEqual([['later']]);

    slow.answer('earlier');
    await settle();
    expect(apply.mock.calls).toEqual([['later']]);

    // And a read begun after the later one still lands.
    read.mockResolvedValueOnce('latest');
    await expect(k.refresh()).resolves.toBe('applied');
    expect(apply.mock.calls).toEqual([['later'], ['latest']]);
  });

  // A failed later read leaves nothing newer standing, so the earlier answer
  // is still the newest there is.
  it('applies an earlier read that lands after a later one failed', async () => {
    const slow = pendingRead();
    read.mockReturnValueOnce(slow.promise).mockRejectedValueOnce(new TypeError('Failed to fetch'));
    const k = kept();
    k.keep();
    await expect(k.refresh()).resolves.toBe('failed');

    slow.answer('earlier');
    await settle();

    expect(apply.mock.calls).toEqual([['earlier']]);
  });

  it('drops a read still out when it is overtaken, as by a write', async () => {
    const out = pendingRead();
    read.mockReturnValueOnce(out.promise);
    const k = kept();
    k.keep();
    k.overtake();

    out.answer('from before the write');
    await settle();

    expect(apply).not.toHaveBeenCalled();
  });

  it('stops: no read on a later opening, and a read still out is dropped', async () => {
    const out = pendingRead();
    read.mockReturnValueOnce(out.promise);
    const stop = kept().keep();
    stop();

    openStream();
    out.answer('asked for by a store that stopped');
    await settle();

    expect(read).toHaveBeenCalledTimes(1);
    expect(apply).not.toHaveBeenCalled();
  });

  it('says what became of a read, and never rejects', async () => {
    const k = kept();
    await expect(k.refresh()).resolves.toBe('applied');
    read.mockRejectedValueOnce(new Error('daemon gone'));
    await expect(k.refresh()).resolves.toBe('failed');
    apply.mockImplementationOnce(() => {
      throw new Error('store refused it');
    });
    await expect(k.refresh()).resolves.toBe('failed');
  });
});

// While the stream is open the daemon answers, so a read that fails then (a
// request that timed out, a 5xx) is asked again by keptCurrent itself: the
// one timer a store's reads have.
describe('keptCurrent asking a failed read again while the stream is open', () => {
  const read = vi.fn<() => Promise<string>>();
  const apply = vi.fn();
  const refused = () => Promise.reject(new Error('gglib did not answer in 30 seconds.'));
  const advance = (ms: number) => vi.advanceTimersByTimeAsync(ms);

  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(Math, 'random').mockReturnValue(0);
    openListeners.clear();
    closeListeners.clear();
    stream.open = false;
    read.mockReset().mockImplementation(refused);
    apply.mockReset();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('asks again after 1 s, doubling to at most 30 s, until a read lands, and then no more', async () => {
    stream.open = true;
    keptCurrent({ read, apply }).keep();
    await advance(0);
    expect(read).toHaveBeenCalledTimes(1);

    const waits = [1000, 2000, 4000, 8000, 16_000, 30_000, 30_000];
    for (const [i, wait] of waits.entries()) {
      await advance(wait - 1);
      expect(read).toHaveBeenCalledTimes(i + 1);
      await advance(1);
      expect(read).toHaveBeenCalledTimes(i + 2);
    }

    read.mockResolvedValue('at last');
    await advance(30_000);
    expect(apply).toHaveBeenCalledWith('at last');
    await advance(10 * 60_000);
    expect(read).toHaveBeenCalledTimes(waits.length + 2);
  });

  it('asks nothing again while the stream is down', async () => {
    keptCurrent({ read, apply }).keep();
    await advance(10 * 60_000);
    expect(read).toHaveBeenCalledTimes(1);
  });

  it('starts asking again once the stream opens and its read fails too', async () => {
    keptCurrent({ read, apply }).keep();
    await advance(0);
    openStream();
    await advance(0);
    expect(read).toHaveBeenCalledTimes(2);
    await advance(1000);
    expect(read).toHaveBeenCalledTimes(3);
  });

  it('stops asking when the stream closes, and its next opening reads anyway', async () => {
    stream.open = true;
    keptCurrent({ read, apply }).keep();
    await advance(0);
    closeStream();
    await advance(10 * 60_000);
    expect(read).toHaveBeenCalledTimes(1);

    read.mockResolvedValue('reopened');
    openStream();
    await advance(0);
    expect(read).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenCalledWith('reopened');
  });

  it('stops asking when the store is released', async () => {
    stream.open = true;
    const stop = keptCurrent({ read, apply }).keep();
    await advance(0);
    stop();
    await advance(10 * 60_000);
    expect(read).toHaveBeenCalledTimes(1);
  });

  it('keeps one retry pending however many reads fail', async () => {
    stream.open = true;
    const k = keptCurrent({ read, apply });
    k.keep();
    await advance(0);
    await k.refresh();
    await k.refresh();
    await Promise.all([k.refresh(), k.refresh(), k.refresh()]);
    expect(read).toHaveBeenCalledTimes(6);

    await advance(1000);
    expect(read).toHaveBeenCalledTimes(7);
  });

  it('drops a pending retry when an event or a write overtakes it', async () => {
    stream.open = true;
    const k = keptCurrent({ read, apply });
    k.keep();
    await advance(0);
    k.overtake();
    await advance(10 * 60_000);
    expect(read).toHaveBeenCalledTimes(1);
  });
});
