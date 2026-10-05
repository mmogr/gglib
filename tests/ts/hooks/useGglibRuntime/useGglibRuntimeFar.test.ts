/**
 * The far machine's chats, read and carried on through this machine's
 * daemon: opening reads the far rows, a send is the new turn alone, its
 * text and its images (the images tested in `useGglibRuntimeImages.test.ts`),
 * the far machine runs and saves the reply, and nothing of it is kept here.
 * A turn that changes the chat's Thinking choice says that too, and no other
 * turn says anything of it; the caller is told once that machine has accepted
 * such a turn, and never for one it refused. Each reading of the chat is
 * handed up, which is where its settings come from.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';
import { mount, send, shown } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';
import { useGglibRuntime, type UseGglibRuntimeOptions } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';
import type { HubChatOpen } from '../../../../src/types/generated/HubChatOpen';
import type { Thinking } from '../../../../src/types/generated/Thinking';

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

  it('a far send with no image is its new text alone, and the reply is what the far machine saved', async () => {
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
    expect(Object.keys(put.body as object)).toEqual(['content']);

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

  it('makes no chat on the far machine: a send with none open sends nothing, and the text comes back', async () => {
    const onError = vi.fn();
    const hook = await mount({ source: 'far', onError });
    send(hook, 'hello');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('A chat on the other machine is started there.');
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('hello');
    expect(daemons.farRequests.filter((r) => r.method !== 'GET')).toEqual([]);
    nothingHere();
  });

  it('a send refused because a far reply is still running sends nothing, and reads that reply', async () => {
    const onError = vi.fn();
    daemons.hub.save(1, { role: 'user', content: 'q' });
    daemons.hub.running('r-far', 1, [{ type: 'text_delta', content: 'Hel' }]);
    // Opening cannot learn whether a reply is running: the listing fails once.
    daemons.listFails = 1;
    const hook = await mount({ ...far(1), onError });
    await waitFor(() => expect(onError).toHaveBeenCalledTimes(1));

    send(hook, 'again');

    await waitFor(() => expect(onError).toHaveBeenCalledTimes(2));
    expect(onError.mock.calls[1][0].message).toBe('Nothing was sent: a reply is still running in this conversation.');
    expect(daemons.farCount('PUT', '/api/remote/chats/')).toBe(0);
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('again');
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/runs/r-far/events')).toBe(1));
    nothingHere();
  });

  it.each([
    [422, 'no_model', 'the chat names no model the other machine has'],
    [403, 'device_not_paired', 'this device is not paired with the other machine'],
    [503, 'unavailable', 'the other machine did not answer'],
  ])('a far send refused with %i says the far sentence and gives the text back', async (status, type, error) => {
    const onError = vi.fn();
    daemons.hub.refuseNext = { status, type, error };
    const hook = await mount({ ...far(1), onError });
    send(hook, 'hello');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe(error);
    expect(hook.result.current.isRunning).toBe(false);
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('hello');
    nothingHere();
  });

  it('a far send while the far machine is not reachable says why and gives the text back', async () => {
    const onError = vi.fn();
    daemons.hub.refuseNext = {
      status: 409,
      type: 'conflict',
      error: 'not connected to a remote machine — `gglib remote join` first',
    };
    const hook = await mount({ ...far(1), onError });
    send(hook, 'hello');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toContain('`gglib remote join`');
    expect(hook.result.current.isRunning).toBe(false);
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('hello');
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

describe('useGglibRuntime on the far machine, the Thinking choice', () => {
  /** Send `text`, end the far run with a reply, and give the body the page sent. */
  async function exchange(hook: Awaited<ReturnType<typeof mount>>, text: string) {
    const before = daemons.farCount('PUT', '/api/remote/chats/1/turns/');
    send(hook, text);
    await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(before + 1));
    const run = [...daemons.hub.runs.values()].at(-1)!;
    void daemons.hub.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    return daemons.farRequests.filter((r) => r.method === 'PUT').at(-1)!.body;
  }

  it('an untouched turn is its text alone; one that changes the choice adds off, then default, each once', async () => {
    let said: Thinking | undefined;
    const thinking = vi.fn(() => said && { said, accepted: () => {} });
    const hook = await mount({ ...far(1), thinking });
    expect(thinking).not.toHaveBeenCalled();

    expect(await exchange(hook, 'one')).toEqual({ content: 'one' });
    said = 'off';
    expect(await exchange(hook, 'two')).toEqual({ content: 'two', thinking: 'off' });
    said = undefined;
    const untouched = await exchange(hook, 'three');
    expect(untouched).toEqual({ content: 'three' });
    expect(Object.keys(untouched as object)).toEqual(['content']);
    said = 'default';
    expect(await exchange(hook, 'four')).toEqual({ content: 'four', thinking: 'default' });
    said = undefined;
    expect(await exchange(hook, 'five')).toEqual({ content: 'five' });
    expect(thinking).toHaveBeenCalledTimes(5);
    nothingHere();
  });

  it('a far turn carries none of the device-wide reasoning controls, with or without the choice', async () => {
    localStorage.setItem(
      'gglib.chat.agentOverrides',
      JSON.stringify({ reasoningEffort: 'high', reasoningBudgetTokens: 2048 }),
    );
    try {
      let said: Thinking | undefined;
      const hook = await mount({ ...far(1), thinking: () => said && { said, accepted: () => {} } });
      expect(await exchange(hook, 'one')).toEqual({ content: 'one' });
      said = 'off';
      expect(await exchange(hook, 'two')).toEqual({ content: 'two', thinking: 'off' });
    } finally {
      localStorage.clear();
    }
  });

  it('calls the choice accepted once that machine has taken the turn, while its reply is still being written, and not for a turn it refused', async () => {
    const onError = vi.fn();
    const accepted = vi.fn();
    daemons.hub.save(1, { role: 'user', content: 'Why did the build break?' });
    const hook = await mount({ ...far(1), thinking: () => ({ said: 'default', accepted }), onError });

    daemons.hub.refuseNext = { status: 503, type: 'unavailable', error: 'the model is not ready' };
    send(hook, 'one');
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(daemons.hub.runs.size).toBe(0);
    expect(accepted).not.toHaveBeenCalled();
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    send(hook, 'one');
    await waitFor(() => expect(accepted).toHaveBeenCalledTimes(1));
    const run = daemons.hub.only();
    expect(daemons.farRequests.filter((r) => r.method === 'PUT').at(-1)!.body).toEqual({ content: 'one', thinking: 'default' });
    expect(hook.result.current.isRunning).toBe(true);

    void daemons.hub.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(accepted).toHaveBeenCalledTimes(1);
    nothingHere();
  });

  it('hands up the far chat each time it is read: at opening, and again after a run', async () => {
    daemons.hub.save(1, { role: 'user', content: 'Why did the build break?' });
    const onFarOpened = vi.fn<(open: HubChatOpen) => void>();
    const hook = await mount({ ...far(1), onFarOpened });
    await waitFor(() => expect(onFarOpened).toHaveBeenCalledTimes(1));
    expect(onFarOpened.mock.calls[0][0].conversation).toMatchObject({ id: 1, system_prompt: 'You are the hub.' });
    expect(onFarOpened.mock.calls[0][0].messages.map((m) => m.content)).toEqual(['Why did the build break?']);

    await exchange(hook, 'And how do I fix it?');
    expect(onFarOpened).toHaveBeenCalledTimes(2);
    expect(onFarOpened.mock.calls[1][0].messages.map((m) => m.content)).toEqual([
      'Why did the build break?',
      'And how do I fix it?',
      'ok',
    ]);
    nothingHere();
  });

  it('hands up nothing of a far chat that was left before its reading came back', async () => {
    // Chat 1 answers only when the test lets it; chat 2 answers at once.
    let answer: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      answer = resolve;
    });
    let asked = 0;
    vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      if (String(input) === '/api/remote/chats/1') {
        asked += 1;
        await held;
      }
      return daemons.fetch(input, init);
    }));
    const onFarOpened = vi.fn<(open: HubChatOpen) => void>();
    const hook = renderHook((props: UseGglibRuntimeOptions) => useGglibRuntime(props), {
      initialProps: { ...far(1), onFarOpened },
    });
    await waitFor(() => expect(asked).toBe(1));

    hook.rerender({ ...far(2), onFarOpened });
    await waitFor(() => expect(onFarOpened).toHaveBeenCalledTimes(1));
    expect(onFarOpened.mock.calls[0][0].conversation.id).toBe(2);

    // Chat 1's answer arrives now, for a reading that was left.
    await act(async () => {
      answer();
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(daemons.farCount('GET', '/api/remote/chats/1')).toBe(1);
    expect(onFarOpened).toHaveBeenCalledTimes(1);
  });

  it('this machine\'s chat hands up nothing: its settings are in its list', async () => {
    const onFarOpened = vi.fn();
    const hook = await mount({ conversationId: 1, selectedServerPort: 9000, onFarOpened });
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(onFarOpened).not.toHaveBeenCalled();
  });
});
