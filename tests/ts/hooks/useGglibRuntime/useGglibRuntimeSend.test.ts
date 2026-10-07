/**
 * A send, as the daemon sees it: one run started under a minted id in a
 * conversation that exists, no turn saved by the page, and the reply shown
 * as the daemon saved it once the run ends.
 *
 * A chat with the paired machine's model goes through the same door, naming
 * that model by its machine and its id there; a conversation it makes is
 * made for that model, so its machine is fixed from the start.
 *
 * A send names no iteration limit: the daemon resolves it from the stored
 * setting, so the page and a paired device run with the same one.
 *
 * A send says the chat's Thinking choice when the caller's `thinking` gives
 * one, asked as the send starts, and the device-wide reasoning controls go
 * with it as they always have. The caller is told once the daemon has
 * accepted the run that said it, and never for a run it refused.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeDaemon } from '../../fixtures/fakeDaemon';
import { conversation, mount, send, shown } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';
import type { ModelRef } from '../../../../src/types/generated/ModelRef';
import type { Thinking } from '../../../../src/types/generated/Thinking';

let daemon: FakeDaemon;

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const local = { conversationId: 1, conversation: conversation(1), selectedServerPort: 9000 };
const far: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };

describe('useGglibRuntime send', () => {
  it('starts one run in the open conversation, and the page saves no turn', async () => {
    const hook = await mount(local);
    send(hook, 'hello');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const run = daemon.only();
    expect(run.info.id).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(daemon.requests.find((r) => r.method === 'PUT')!.url).toBe(
      `/api/runs/${run.info.id}?kind=agent`,
    );
    expect(run.request).toMatchObject({
      conversation_id: 1,
      messages: [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: 'hello' },
      ],
    });

    daemon.emit(run.info.id, { type: 'text_delta', content: 'Hi' });
    await waitFor(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('Hi'));
    expect(hook.result.current.isRunning).toBe(true);

    daemon.emit(run.info.id, { type: 'final_answer', content: 'Hi there' });
    daemon.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'Hi there' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    // What the daemon saved, with its ids: each message once.
    expect(shown(hook.result.current.messages)).toEqual([
      ['system-1', 'system', 'You are a helpful assistant.'],
      ['db-1', 'user', 'hello'],
      ['db-2', 'assistant', 'Hi there'],
    ]);
    expect(daemon.count('POST', '/api/messages')).toBe(0);
    expect(daemon.count('PUT', '/api/messages')).toBe(0);
  });

  it('names no iteration limit, alone or beside the limits of the Tools popover: the daemon takes the stored one', async () => {
    const hook = await mount(local);
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request!.config).toBeNull();
    daemon.finish(daemon.only().info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    localStorage.setItem('gglib.chat.agentOverrides', JSON.stringify({ maxParallelTools: 4 }));
    try {
      send(hook, 'again');
      await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(2));
      expect([...daemon.runs.values()].at(-1)!.request!.config).toEqual({
        max_iterations: null,
        max_parallel_tools: 4,
        tool_timeout_ms: null,
        observation_tools: null,
        max_observation_steps: null,
      });
    } finally {
      localStorage.removeItem('gglib.chat.agentOverrides');
    }
  });

  it('creates the conversation first when none is open, so the run has one', async () => {
    const onConversationChanged = vi.fn();
    const hook = await mount({ selectedServerPort: 9000, onConversationChanged });
    send(hook, 'hello');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const order = daemon.requests.map((r) => `${r.method} ${r.url.split('?')[0]}`);
    expect(order.indexOf('POST /api/conversations')).toBeLessThan(
      order.findIndex((r) => r.startsWith('PUT /api/runs/')),
    );
    expect(daemon.only().request!.conversation_id).toBe(100);
    expect(onConversationChanged).toHaveBeenCalledWith(100);
  });

  it('a second send while a run is live starts nothing', async () => {
    const hook = await mount(local);
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));

    send(hook, 'again');
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(daemon.count('PUT', '/api/runs/')).toBe(1);
    expect(daemon.saved(1).map((r) => r.content)).toEqual(['hello']);
  });

  it('a turn on a far model goes through the same door, by its ref and with no local server', async () => {
    const hook = await mount({ conversationId: 1, conversation: conversation(1), pairedModel: far });
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ far, model: null, conversation_id: 1 });
  });

  it('the first turn on a far model makes its conversation for that model', async () => {
    const hook = await mount({ pairedModel: far });
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const made = daemon.requests.find((r) => r.method === 'POST' && r.url === '/api/conversations');
    expect(made?.body).toMatchObject({ model: far, model_id: null });
    expect(daemon.only().request).toMatchObject({ far });
  });

  it('the conversation a local turn makes is made for no model', async () => {
    const hook = await mount({ selectedServerPort: 9000 });
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const made = daemon.requests.find((r) => r.method === 'POST' && r.url === '/api/conversations');
    expect(made?.body).toMatchObject({ model: null });
    expect(daemon.only().request).toMatchObject({ far: null });
  });

  it('a local turn with no server selected never reaches the wire', async () => {
    const onError = vi.fn();
    const hook = await mount({ conversationId: 1, conversation: conversation(1), onError });
    send(hook, 'hello');
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('No server selected. Please serve a model first.');
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
  });

  it.each([
    [429, 'agent_busy', 'all agent loop slots are in use; try again later'],
    [404, 'conversation_not_found', 'no conversation has id 1'],
    [503, 'unavailable', 'the model is not ready'],
  ])('a start refused with %i shows why, what is saved, and gives the text back', async (status, type, error) => {
    const onError = vi.fn();
    daemon.refuseNext = { status, type, error };
    const hook = await mount({ ...local, onError });
    send(hook, 'hello');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe(error);
    expect(hook.result.current.isRunning).toBe(false);
    expect(shown(hook.result.current.messages)).toEqual([
      ['system-1', 'system', 'You are a helpful assistant.'],
    ]);
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('hello');
  });

  it('a failed run shows its error after what the daemon saved', async () => {
    const onError = vi.fn();
    const hook = await mount({ ...local, onError });
    send(hook, 'hello');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const { id } = daemon.only().info;

    daemon.emit(id, { type: 'text_delta', content: 'par' });
    daemon.emit(id, { type: 'error', message: 'the model went away' });
    daemon.finish(id, 'failed', [{ role: 'assistant', content: 'par', metadata: { incomplete: true } }]);

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('Agent loop error: the model went away');
    const last = hook.result.current.messages.at(-1)!;
    expect(last.id).toBe('db-2');
    expect(last.status).toEqual({ type: 'incomplete', reason: 'cancelled' });
  });

  it('keeps nothing of the run in browser storage, and logs nothing new', async () => {
    const consoleSpies = (['log', 'info', 'warn', 'error', 'debug'] as const).map((level) =>
      vi.spyOn(console, level).mockImplementation(() => {}),
    );
    const before = [{ ...localStorage }, { ...sessionStorage }];

    const hook = await mount(local);
    send(hook, 'a private question');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a private answer' });
    daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a private answer' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect([{ ...localStorage }, { ...sessionStorage }]).toEqual(before);
    consoleSpies.forEach((spy) => {
      expect(spy).not.toHaveBeenCalled();
      spy.mockRestore();
    });
  });
});

describe('useGglibRuntime send, the Thinking choice', () => {
  afterEach(() => localStorage.clear());

  /** Send `text`, end its run with a reply, and give the body the daemon got. */
  async function exchange(hook: Awaited<ReturnType<typeof mount>>, text: string) {
    const before = daemon.count('PUT', '/api/runs/');
    send(hook, text);
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(before + 1));
    const run = [...daemon.runs.values()].at(-1)!;
    daemon.emit(run.info.id, { type: 'final_answer', content: 'ok' });
    void daemon.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    return run.request as unknown as Record<string, unknown>;
  }

  it('says what `thinking` answers as each send starts: off once, then nothing, default once, then nothing', async () => {
    let said: Thinking | undefined;
    const thinking = vi.fn(() => said && { said, accepted: () => {} });
    const hook = await mount({ ...local, thinking });
    // Asked at a send, never at mount.
    expect(thinking).not.toHaveBeenCalled();

    expect(Object.keys(await exchange(hook, 'one'))).not.toContain('thinking');
    said = 'off';
    expect((await exchange(hook, 'two')).thinking).toBe('off');
    said = undefined;
    expect(Object.keys(await exchange(hook, 'three'))).not.toContain('thinking');
    said = 'default';
    expect((await exchange(hook, 'four')).thinking).toBe('default');
    said = undefined;
    expect(Object.keys(await exchange(hook, 'five'))).not.toContain('thinking');
    expect(thinking).toHaveBeenCalledTimes(5);
  });

  it('says nothing of thinking when the caller gives no `thinking` at all', async () => {
    const hook = await mount(local);
    expect(Object.keys(await exchange(hook, 'hello'))).not.toContain('thinking');
  });

  it('sends the device-wide effort and budget unchanged, whatever the choice says', async () => {
    localStorage.setItem(
      'gglib.chat.agentOverrides',
      JSON.stringify({ reasoningEffort: 'high', reasoningBudgetTokens: 2048 }),
    );
    let said: Thinking | undefined;
    const hook = await mount({ ...local, thinking: () => said && { said, accepted: () => {} } });

    const device = { reasoning_effort: 'high', reasoning_budget_tokens: 2048 };
    expect(await exchange(hook, 'one')).toMatchObject(device);
    said = 'off';
    expect(await exchange(hook, 'two')).toMatchObject({ ...device, thinking: 'off' });
    said = 'default';
    expect(await exchange(hook, 'three')).toMatchObject({ ...device, thinking: 'default' });
  });

  it('a send gglib refused is asked again at the next send, and its choice is not called accepted: it is the caller\'s to keep', async () => {
    const onError = vi.fn();
    const accepted = vi.fn();
    const thinking = vi.fn(() => ({ said: 'off' as Thinking, accepted }));
    daemon.refuseNext = { status: 503, type: 'unavailable', error: 'the model is not ready' };
    const hook = await mount({ ...local, thinking, onError });
    send(hook, 'hello');
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(daemon.runs.size).toBe(0);
    expect(accepted).not.toHaveBeenCalled();

    expect((await exchange(hook, 'hello')).thinking).toBe('off');
    expect(thinking).toHaveBeenCalledTimes(2);
    expect(accepted).toHaveBeenCalledTimes(1);
  });

  it('calls the choice accepted as soon as the daemon has taken the run that said it, while its reply is still being written', async () => {
    const accepted = vi.fn();
    const hook = await mount({ ...local, thinking: () => ({ said: 'off', accepted }) });
    send(hook, 'hello');
    await waitFor(() => expect(accepted).toHaveBeenCalledTimes(1));
    const run = daemon.only();
    expect(run.request).toMatchObject({ thinking: 'off' });
    expect(run.info.status).not.toBe('completed');
    expect(hook.result.current.isRunning).toBe(true);

    void daemon.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(accepted).toHaveBeenCalledTimes(1);
  });

  it('calls the choice accepted for a run the daemon took after the conversation was left', async () => {
    // The daemon answers the start only when the test lets it.
    let answer: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      answer = resolve;
    });
    vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'PUT' && String(input).startsWith('/api/runs/')) await held;
      return daemon.fetch(input, init);
    }));
    const accepted = vi.fn();
    const thinking = () => ({ said: 'off' as Thinking, accepted });
    const hook = await mount({ ...local, thinking });
    send(hook, 'hello');
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    // Another conversation is opened before the daemon answers.
    hook.rerender({ ...local, conversationId: 2, conversation: conversation(2), thinking });
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(daemon.runs.size).toBe(0);
    expect(accepted).not.toHaveBeenCalled();

    await act(async () => {
      answer();
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(daemon.only().request).toMatchObject({ conversation_id: 1, thinking: 'off' });
    expect(accepted).toHaveBeenCalledTimes(1);
  });

  it('a turn on a far model says it through the same door', async () => {
    const accepted = vi.fn();
    const hook = await mount({ conversationId: 1, conversation: conversation(1), pairedModel: far, thinking: () => ({ said: 'off', accepted }) });
    expect(await exchange(hook, 'hello')).toMatchObject({ far, thinking: 'off' });
    expect(accepted).toHaveBeenCalledTimes(1);
  });
});
