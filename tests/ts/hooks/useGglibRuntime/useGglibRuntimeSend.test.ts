/**
 * A send, as the daemon sees it: one run started under a minted id in a
 * conversation that exists, no turn saved by the page, and the reply shown
 * as the daemon saved it once the run ends.
 *
 * A chat with the paired machine's model goes through the same door, naming
 * that model by its machine and its id there; a conversation it makes is
 * made for that model, so its machine is fixed from the start.
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
