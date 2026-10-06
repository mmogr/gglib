/**
 * The one bridge from a category of daemon events into a registry.
 *
 * The server, proxy and remote registries are all fed through it, and what it
 * owns is the order: subscribe before the read, and apply a read only if
 * nothing happened while it was out — no event, and no cleanup. Each of the
 * three has its own test of that through its own bridge; these hold the
 * sequence itself, where it is written.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { ProxyEvent } from '../../../src/services/transport/types/events';

const subscribe = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ subscribe }),
}));

import { bridgeEvents } from '../../../src/services/bridgeEvents';

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
});
