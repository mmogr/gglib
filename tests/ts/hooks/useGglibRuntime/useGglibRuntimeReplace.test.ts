/**
 * An edit and a regenerate replace saved rows through the run itself: the
 * page deletes nothing, the daemon replaces the rows only once it accepts
 * the run, and a refusal of any kind leaves every row where it was.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeDaemon } from '../../fixtures/fakeDaemon';
import { conversation, edit, mount, regenerate, shown } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';

let daemon: FakeDaemon;

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
  daemon.save(1, { role: 'user', content: 'q' });
  daemon.save(1, { role: 'assistant', content: 'a' });
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const open = { conversationId: 1, conversation: conversation(1), selectedServerPort: 9000 };

/** What conversation 1 holds, as the thread shows it and as it is saved. */
function held(hook: Awaited<ReturnType<typeof mount>>) {
  return {
    saved: daemon.saved(1).map((r) => [r.role, r.content]),
    shown: shown(hook.result.current.messages).map(([, role, text]) => [role, text]),
  };
}

describe('useGglibRuntime edit and regenerate', () => {
  it('an edit reaches the database: its turn replaced by the new message, once', async () => {
    const hook = await mount(open);
    edit(hook, 'system-1', 'q2');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ replace_from: 1 });
    expect(daemon.count('DELETE', '/api/messages')).toBe(0);

    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a2' });
    void daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a2' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(held(hook)).toEqual({
      saved: [['user', 'q2'], ['assistant', 'a2']],
      shown: [['system', 'You are a helpful assistant.'], ['user', 'q2'], ['assistant', 'a2']],
    });
  });

  it('regenerate holds the question once, with the new reply', async () => {
    const hook = await mount(open);
    regenerate(hook, 'db-1');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ replace_from: 1 });
    expect(daemon.only().request!.messages.at(-1)).toEqual({ role: 'user', content: 'q' });

    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a again' });
    void daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a again' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(held(hook)).toEqual({
      saved: [['user', 'q'], ['assistant', 'a again']],
      shown: [['system', 'You are a helpful assistant.'], ['user', 'q'], ['assistant', 'a again']],
    });
  });

  it('an edit and a regenerate say the Thinking choice as a send does', async () => {
    const accepted = vi.fn();
    const hook = await mount({ ...open, thinking: () => ({ said: 'off', accepted }) });
    regenerate(hook, 'db-1');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ replace_from: 1, thinking: 'off' });
    const { id } = daemon.only().info;
    void daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a again' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(accepted).toHaveBeenCalledTimes(1);

    edit(hook, 'system-1', 'q2');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(2));
    expect([...daemon.runs.values()].at(-1)!.request).toMatchObject({ thinking: 'off' });
    await waitFor(() => expect(accepted).toHaveBeenCalledTimes(2));
  });

  const refusals: Array<[string, (d: FakeDaemon) => void, string]> = [
    ['busy (429)', (d) => (d.refuseNext = { status: 429, type: 'agent_busy', error: 'all agent loop slots are in use; try again later' }), 'all agent loop slots are in use; try again later'],
    ['unavailable (503)', (d) => (d.refuseNext = { status: 503, type: 'unavailable', error: 'the model is not ready' }), 'the model is not ready'],
    ['a network error', (d) => (d.dropNextStart = true), 'Failed to fetch'],
  ];

  it.each(refusals)('an edit refused as %s changes nothing and gives the text back', async (_, refuse, why) => {
    const onError = vi.fn();
    const hook = await mount({ ...open, onError });
    refuse(daemon);
    edit(hook, 'system-1', 'q2');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe(why);
    await waitFor(() => expect(held(hook).shown).toHaveLength(3));
    expect(held(hook)).toEqual({
      saved: [['user', 'q'], ['assistant', 'a']],
      shown: [['system', 'You are a helpful assistant.'], ['user', 'q'], ['assistant', 'a']],
    });
    expect(hook.result.current.runtime.thread.composer.getState().text).toBe('q2');
    expect(hook.result.current.isRunning).toBe(false);
  });

  it.each(refusals)('a regenerate refused as %s changes nothing', async (_, refuse, why) => {
    const onError = vi.fn();
    const hook = await mount({ ...open, onError });
    refuse(daemon);
    regenerate(hook, 'db-1');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe(why);
    await waitFor(() => expect(held(hook).shown).toHaveLength(3));
    expect(held(hook)).toEqual({
      saved: [['user', 'q'], ['assistant', 'a']],
      shown: [['system', 'You are a helpful assistant.'], ['user', 'q'], ['assistant', 'a']],
    });
    expect(hook.result.current.isRunning).toBe(false);
  });
});
