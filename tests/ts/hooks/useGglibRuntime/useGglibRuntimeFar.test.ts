/**
 * The far machine's chats, read and carried on through this machine's
 * daemon: opening reads the far rows, a send is the new text alone, the
 * far machine runs and saves the reply, and nothing of it is kept here.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';
import { mount, send, shown } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

/** Far chat `id`, as the page opens it: no local server, no local conversation. */
const far = (id: number) => ({ conversationId: id, source: 'far' as const });

/** Nothing went to this machine's own chat or run routes. */
function nothingHere() {
  expect(daemons.here.requests.map((r) => `${r.method} ${r.url}`)).toEqual([]);
}

describe('useGglibRuntime on the far machine', () => {
  it('opening a far chat reads its rows from the far machine, and asks nothing here', async () => {
    daemons.hub.save(1, { role: 'user', content: 'Why did the build break?' });
    daemons.hub.save(1, { role: 'assistant', content: 'A dependency moved.' });

    const hook = await mount(far(1));

    await waitFor(() =>
      expect(shown(hook.result.current.messages)).toEqual([
        ['system-1', 'system', 'You are the hub.'],
        ['db-1', 'user', 'Why did the build break?'],
        ['db-2', 'assistant', 'A dependency moved.'],
      ]),
    );
    expect(daemons.farCount('GET', '/api/remote/chats/1')).toBe(1);
    nothingHere();
  });

  it('a far send is the new text alone, and the reply is what the far machine saved', async () => {
    daemons.hub.save(1, { role: 'user', content: 'Why did the build break?' });
    daemons.hub.save(1, { role: 'assistant', content: 'A dependency moved.' });
    const hook = await mount(far(1));
    send(hook, 'And how do I fix it?');

    await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(1));
    const put = daemons.farRequests.find((r) => r.method === 'PUT')!;
    const run = daemons.hub.only();
    expect(put.url).toBe(`/api/remote/chats/1/turns/${run.info.id}`);
    expect(run.info.id).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(put.body).toEqual({ content: 'And how do I fix it?' });

    daemons.hub.emit(run.info.id, { type: 'text_delta', content: 'Pin' });
    await waitFor(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('Pin'));
    expect(daemons.farCount('GET', `/api/remote/runs/${run.info.id}/events`)).toBe(1);

    daemons.hub.emit(run.info.id, { type: 'final_answer', content: 'Pin it.' });
    daemons.hub.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'Pin it.' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(shown(hook.result.current.messages).slice(1)).toEqual([
      ['db-1', 'user', 'Why did the build break?'],
      ['db-2', 'assistant', 'A dependency moved.'],
      ['db-3', 'user', 'And how do I fix it?'],
      ['db-4', 'assistant', 'Pin it.'],
    ]);
    nothingHere();
  });

  it('opening a far chat with a reply going reads it, found by the far listing', async () => {
    daemons.hub.save(1, { role: 'user', content: 'q' });
    daemons.hub.running('r-far', 1, [{ type: 'text_delta', content: 'Hel' }]);

    const hook = await mount(far(1));

    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));
    await waitFor(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('Hel'));
    expect(daemons.farCount('GET', '/api/remote/chats')).toBeGreaterThan(0);
    expect(daemons.farCount('GET', '/api/remote/runs/r-far/events')).toBe(1);
    nothingHere();
  });

  it('Stop cancels the far run', async () => {
    daemons.hub.save(1, { role: 'user', content: 'q' });
    daemons.hub.running('r-far', 1, [{ type: 'text_delta', content: 'Hel' }]);
    const hook = await mount(far(1));
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    act(() => hook.result.current.runtime.thread.cancelRun());

    await waitFor(() => expect(daemons.farCount('POST', '/api/remote/runs/r-far/cancel')).toBe(1));
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(hook.result.current.messages.at(-1)!.status).toEqual({ type: 'incomplete', reason: 'cancelled' });
    nothingHere();
  });

  it('offers no edit and no regenerate on a far chat, and both on this machine’s', async () => {
    const farHook = await mount(far(1));
    const { capabilities } = farHook.result.current.runtime.thread.getState();
    expect([capabilities.edit, capabilities.reload]).toEqual([false, false]);

    const hereHook = await mount({ conversationId: 1, selectedServerPort: 9000 });
    const here = hereHook.result.current.runtime.thread.getState().capabilities;
    expect([here.edit, here.reload]).toEqual([true, true]);
  });

  it('makes no chat on the far machine: a send with none open sends nothing', async () => {
    const onError = vi.fn();
    const hook = await mount({ source: 'far', onError });
    send(hook, 'hello');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(daemons.farRequests.filter((r) => r.method !== 'GET')).toEqual([]);
    nothingHere();
  });

  it('keeps nothing of the far chat in browser storage', async () => {
    const before = [{ ...localStorage }, { ...sessionStorage }];
    daemons.hub.save(1, { role: 'user', content: 'a private question' });
    daemons.hub.save(1, { role: 'assistant', content: 'a private answer' });

    const hook = await mount(far(1));
    await waitFor(() => expect(hook.result.current.messages).toHaveLength(3));
    send(hook, 'another private question');
    await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(1));
    const { id } = daemons.hub.only().info;
    daemons.hub.emit(id, { type: 'final_answer', content: 'another private answer' });
    daemons.hub.finish(id, 'completed', [{ role: 'assistant', content: 'another private answer' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect([{ ...localStorage }, { ...sessionStorage }]).toEqual(before);
  });
});
