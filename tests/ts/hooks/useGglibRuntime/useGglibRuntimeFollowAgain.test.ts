/**
 * A reply's stream that ends before its run does (a proxy that hiccups, a
 * service that restarts its listener, a machine that slept) is read again
 * from where it stopped while the run is live: the reply goes on, with no
 * text shown twice and none lost. The tries back off and stop after a stated
 * number, and the page then says so and asks before the next send. A run
 * that ended is never read again, and leaving stops at once.
 *
 * Each test opens on real timers, then holds the clock: the waits between
 * tries are told apart to the millisecond.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { FakeDaemon } from '../../fixtures/fakeDaemon';
import { conversation, mount, send, shown } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';

let daemon: FakeDaemon;

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const open = (id: number) => ({ conversationId: id, conversation: conversation(id) });

/** The waits between tries, in order, and how many tries there are. */
const WAITS = [250, 500, 1000, 2000, 4000, 4000, 4000, 4000];

/** Move the held clock on by `ms`, and let what that releases run. */
async function tick(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Let the page catch up with the held clock still, until `check` passes. */
async function until(check: () => void) {
  for (let i = 0; ; i++) {
    try {
      check();
      return;
    } catch (error) {
      if (i >= 50) throw error;
      await tick(0);
    }
  }
}

/** The readings of run r1's stream, as asked. */
const reads = () => daemon.requests.filter((r) => r.url.startsWith('/api/runs/r1/events')).map((r) => r.url);

/** The thread's assistant messages, by their text. */
type Hook = Awaited<ReturnType<typeof mount>>;
const replies = (hook: Hook) => shown(hook.result.current.messages).filter(([, role]) => role === 'assistant').map(([, , text]) => text);

/** Conversation 1 open with run r1 drawn as far as "Hel", then the clock held. */
async function following(onError = vi.fn()) {
  daemon.save(1, { role: 'user', content: 'q' });
  daemon.running('r1', 1, [{ type: 'text_delta', content: 'Hel' }]);
  const hook = await mount({ ...open(1), onError });
  await waitFor(() => expect(replies(hook)).toEqual(['Hel']));
  expect(hook.result.current.isRunning).toBe(true);
  vi.useFakeTimers();
  return hook;
}

describe('useGglibRuntime and a reply stream that ends before its run', () => {
  for (const how of ['closed', 'broken'] as const) {
    it(`${how}, it is read again from where it stopped, and the reply goes on`, async () => {
      const onError = vi.fn();
      const hook = await following(onError);

      daemon.drop('r1', how);
      await tick();
      // Still the reply being written, with its Stop: the run is live.
      expect(hook.result.current.isRunning).toBe(true);
      expect(replies(hook)).toEqual(['Hel']);

      daemon.emit('r1', { type: 'text_delta', content: 'lo' });
      await tick(WAITS[0] - 1);
      expect(reads()).toHaveLength(1);
      await tick(1);
      expect(reads()).toEqual(['/api/runs/r1/events?after=0', '/api/runs/r1/events?after=1']);

      // What came while it was down is drawn once, after what came before.
      await until(() => expect(replies(hook)).toEqual(['Hello']));
      daemon.emit('r1', { type: 'text_delta', content: '!' });
      await until(() => expect(replies(hook)).toEqual(['Hello!']));

      void daemon.finish('r1', 'completed', [{ role: 'assistant', content: 'Hello!' }]);
      await until(() => expect(hook.result.current.isRunning).toBe(false));
      expect(shown(hook.result.current.messages).map(([, role, text]) => [role, text])).toEqual([
        ['system', 'You are a helpful assistant.'],
        ['user', 'q'],
        ['assistant', 'Hello!'],
      ]);
      expect(hook.result.current.endedRun).toMatchObject({ id: 'r1', status: 'completed' });
      expect(onError).not.toHaveBeenCalled();
    });
  }

  it('a run read to its end is not read again', async () => {
    const hook = await following();
    void daemon.finish('r1', 'completed', [{ role: 'assistant', content: 'Hel' }]);
    await until(() => expect(hook.result.current.isRunning).toBe(false));

    await tick(60_000);
    expect(reads()).toHaveLength(1);
  });

  it('a run that ended while its stream was down is read to its end, and no further', async () => {
    const hook = await following();
    daemon.drop('r1');
    daemon.emit('r1', { type: 'text_delta', content: 'lo' });
    void daemon.finish('r1', 'completed', [{ role: 'assistant', content: 'Hello' }]);
    await tick();
    expect(daemon.runs.get('r1')!.info.status).toBe('completed');
    expect(hook.result.current.isRunning).toBe(true);

    await tick(WAITS[0]);
    await until(() => expect(hook.result.current.isRunning).toBe(false));
    expect(hook.result.current.endedRun).toMatchObject({ id: 'r1', status: 'completed' });
    expect(replies(hook)).toEqual(['Hello']);

    await tick(60_000);
    expect(reads()).toHaveLength(2);
  });

  it('tries that fail wait longer each time, and stop after the last, saying so', async () => {
    const onError = vi.fn();
    const hook = await following(onError);
    let down = true;
    daemon.before = (method, path) => {
      if (down && method === 'GET' && path === '/api/runs/r1/events') throw new TypeError('Failed to fetch');
    };

    daemon.drop('r1');
    await tick();
    for (const [i, wait] of WAITS.entries()) {
      await tick(wait - 1);
      expect(reads()).toHaveLength(1 + i);
      expect(hook.result.current.isRunning).toBe(true);
      await tick(1);
      expect(reads()).toHaveLength(2 + i);
    }

    // The tries are spent: no Stop, the saved rows, and a sentence why.
    await until(() => expect(hook.result.current.isRunning).toBe(false));
    expect(replies(hook)).toEqual([]);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls[0][0].message).toBe(
      'The reply stopped arriving and could not be read again: Failed to fetch. ' +
        'It may still be running; open this conversation again to see it.',
    );
    await tick(60_000);
    expect(reads()).toHaveLength(1 + WAITS.length);

    // The run may still be live, so a send asks first, and finds it.
    down = false;
    vi.useRealTimers();
    send(hook, 'meanwhile');
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));
    expect(onError.mock.calls.at(-1)![0].message).toBe('Nothing was sent: a reply is still running in this conversation.');
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    await waitFor(() => expect(replies(hook)).toEqual(['Hel']));
  });

  it('a try that reads more of the reply starts the waits again', async () => {
    const hook = await following();
    daemon.drop('r1');
    await tick(WAITS[0]);
    expect(reads()).toHaveLength(2);
    daemon.emit('r1', { type: 'text_delta', content: 'lo' });
    await until(() => expect(replies(hook)).toEqual(['Hello']));

    daemon.drop('r1');
    await tick(WAITS[0]);
    expect(reads()).toEqual([
      '/api/runs/r1/events?after=0',
      '/api/runs/r1/events?after=1',
      '/api/runs/r1/events?after=2',
    ]);
  });

  it('a run the service no longer has is not tried again', async () => {
    const onError = vi.fn();
    const hook = await following(onError);
    daemon.drop('r1');
    daemon.vanished.add('r1');

    await tick(WAITS[0]);
    await until(() => expect(hook.result.current.isRunning).toBe(false));
    expect(onError).toHaveBeenCalledWith(expect.objectContaining({ message: 'no run has id r1' }));
    await tick(60_000);
    expect(reads()).toHaveLength(2);
  });

  it('leaving the conversation while a try waits stops it, and none goes out after', async () => {
    daemon.save(2, { role: 'user', content: 'other' });
    const hook = await following();
    daemon.drop('r1');
    await tick(WAITS[0] - 100);

    hook.rerender(open(2));
    await until(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('other'));
    await tick(60_000);
    expect(reads()).toHaveLength(1);
    expect(hook.result.current.isRunning).toBe(false);
  });

  it('closing the page while a try waits stops it, and none goes out after', async () => {
    const hook = await following();
    daemon.drop('r1');
    await tick(WAITS[0] - 100);

    hook.unmount();
    await tick(60_000);
    expect(reads()).toHaveLength(1);
  });

  it('Stop while a try waits cancels the run, and the try reads its end, and no further', async () => {
    const hook = await following();
    daemon.drop('r1');
    await tick();

    act(() => hook.result.current.runtime.thread.cancelRun());
    await until(() => expect(daemon.count('POST', '/api/runs/r1/cancel')).toBe(1));
    await tick(WAITS[0]);
    await until(() => expect(hook.result.current.isRunning).toBe(false));
    const last = hook.result.current.messages.at(-1)!;
    expect(shown([last])).toEqual([['db-2', 'assistant', 'Hel']]);
    expect(last.status).toEqual({ type: 'incomplete', reason: 'cancelled' });

    await tick(60_000);
    expect(reads()).toHaveLength(2);
  });
});
