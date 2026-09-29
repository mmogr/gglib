/**
 * Opening a conversation against a run that may end at any moment of it:
 * the page asks for the live run first, then loads the rows, then reads the
 * run, so the reply is on screen once wherever the end falls. Nothing can
 * be sent before the question is answered, and a run the daemon no longer
 * has leaves the page ready to send.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeDaemon } from '../../fixtures/fakeDaemon';
import { conversation, mount, send, shown } from './runtimeHarness';
import { useGglibRuntime, type UseGglibRuntimeOptions } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';

let daemon: FakeDaemon;

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const open: UseGglibRuntimeOptions = { conversationId: 1, conversation: conversation(1), selectedServerPort: 9000 };

/** A run going in conversation 1, its question saved and its answer drawn so far. */
function live() {
  daemon.save(1, { role: 'user', content: 'q' });
  daemon.running('r1', 1, [
    { type: 'text_delta', content: 'answer' },
    { type: 'final_answer', content: 'answer' },
  ]);
}

const end = () => daemon.finish('r1', 'completed', [{ role: 'assistant', content: 'answer' }]);

/** The first time `path` is asked for, end the run before answering. */
function endAt(path: string) {
  let done = false;
  daemon.before = async (_, asked) => {
    if (!done && asked === path) {
      done = true;
      await end();
    }
  };
}

const once = [
  ['system', 'You are a helpful assistant.'],
  ['user', 'q'],
  ['assistant', 'answer'],
];

async function settles(hook: Awaited<ReturnType<typeof mount>>) {
  await waitFor(() => {
    expect(hook.result.current.isRunning).toBe(false);
    expect(shown(hook.result.current.messages).map(([, role, text]) => [role, text])).toEqual(once);
  });
  expect(hook.result.current.messages.every((m) => /^(db|system)-/.test(m.id ?? ''))).toBe(true);
}

describe('useGglibRuntime opening a conversation while its run ends', () => {
  it('ended before the lookup: the saved reply, once', async () => {
    live();
    await end();
    await settles(await mount(open));
  });

  it('ended as the lookup is asked: the saved reply, once', async () => {
    live();
    endAt('/api/runs');
    await settles(await mount(open));
  });

  it('ended between the lookup and the load: the saved reply, once', async () => {
    live();
    endAt('/api/conversations/1/messages');
    await settles(await mount(open));
    expect(daemon.requests.some((r) => r.url.includes('/events'))).toBe(true);
  });

  it('ended between the load and the attach: the saved reply, once', async () => {
    live();
    endAt('/api/runs/r1/events');
    await settles(await mount(open));
  });

  it('ended during the attach: the saved reply, once', async () => {
    live();
    const hook = await mount(open);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));
    await act(async () => {
      await end();
    });
    await settles(hook);
  });

  it('nothing can be sent until the lookup has answered', async () => {
    let answer = () => {};
    const held = new Promise<void>((r) => {
      answer = r;
    });
    daemon.before = async (method, path) => {
      if (method === 'GET' && path === '/api/runs') await held;
    };
    const hook = renderHook((props: UseGglibRuntimeOptions) => useGglibRuntime(props), { initialProps: open });
    expect(hook.result.current.isLoading).toBe(true);

    send(hook as unknown as Awaited<ReturnType<typeof mount>>, 'too soon');
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    expect(hook.result.current.isLoading).toBe(true);

    answer();
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    send(hook as unknown as Awaited<ReturnType<typeof mount>>, 'now');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
  });

  it('a run the daemon no longer has ends the reading, and a new send works', async () => {
    live();
    daemon.vanished.add('r1');
    const onError = vi.fn();
    const hook = await mount({ ...open, onError });

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('no run has id r1');
    expect(hook.result.current.isRunning).toBe(false);

    send(hook, 'again');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
  });
});
