/**
 * The run outlives the page: coming back to a conversation draws the reply
 * still going in it, leaving stops reading and never the run, Stop cancels
 * it, and an edit or a regenerate leaves each message saved once.
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

let daemon: FakeDaemon;

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const open = (id: number) => ({ conversationId: id, conversation: conversation(id), selectedServerPort: 9000 });

const toolStart = {
  type: 'tool_call_start',
  tool_call: { id: 't1', name: 'read', arguments: {} },
  display_name: 'Read',
};
const toolDone = {
  type: 'tool_call_complete',
  tool_name: 'read',
  result: { tool_call_id: 't1', content: 'ok', success: true },
  wait_ms: 0,
  execute_duration_ms: 1,
  display_name: 'Read',
  duration_display: '1ms',
};

/** Let whatever a leave might still send go out. */
async function settle() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 50));
  });
}

/** The parts of a message that are tool calls, as the thread holds them. */
function toolCalls(message: { content: unknown }) {
  return (message.content as Array<Record<string, unknown>>).filter((p) => p.type === 'tool-call');
}

describe('useGglibRuntime and a run that outlives the page', () => {
  it('coming back draws the live reply from its first event, then shows what was saved', async () => {
    daemon.save(1, { role: 'user', content: 'q' });
    daemon.running('r-live', 1, [
      { type: 'text_delta', content: 'Hel' },
      toolStart,
      toolDone,
      { type: 'iteration_complete', iteration: 1, tool_calls: 1 },
    ]);

    const hook = await mount(open(1));

    // The saved rows first, then the reply as far as it has got.
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));
    await waitFor(() =>
      expect(shown(hook.result.current.messages).slice(0, 3).map(([, role, text]) => [role, text])).toEqual([
        ['system', 'You are a helpful assistant.'],
        ['user', 'q'],
        ['assistant', 'Hel'],
      ]),
    );
    expect(toolCalls(hook.result.current.messages[2])).toMatchObject([{ toolCallId: 't1', result: 'ok' }]);
    const read = daemon.requests.find((r) => r.url.includes('/events'))!;
    expect(read.url).toBe('/api/runs/r-live/events?after=0');

    daemon.emit('r-live', { type: 'final_answer', content: 'lo' });
    daemon.finish('r-live', 'completed', [
      { role: 'assistant', content: 'Hel', metadata: { tool_calls: [{ id: 't1', name: 'read', arguments: {} }] } },
      { role: 'tool', content: 'ok', metadata: { tool_call_id: 't1' } },
      { role: 'assistant', content: 'lo' },
    ]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(shown(hook.result.current.messages)).toEqual([
      ['system-1', 'system', 'You are a helpful assistant.'],
      ['db-1', 'user', 'q'],
      ['db-2', 'assistant', 'Hel'],
      ['db-4', 'assistant', 'lo'],
    ]);
    expect(toolCalls(hook.result.current.messages[2])).toMatchObject([{ toolCallId: 't1', result: 'ok' }]);
  });

  it('a conversation with no live run is only loaded', async () => {
    daemon.save(1, { role: 'user', content: 'q' });
    daemon.running('elsewhere', 2);
    const hook = await mount(open(1));
    await waitFor(() => expect(daemon.count('GET', '/api/runs')).toBe(1));
    expect(hook.result.current.isRunning).toBe(false);
    expect(daemon.requests.some((r) => r.url.includes('/events'))).toBe(false);
  });

  it('closing the page stops reading and leaves the run going', async () => {
    daemon.running('r-live', 1, [{ type: 'text_delta', content: 'Hel' }]);
    const hook = await mount(open(1));
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    hook.unmount();
    await settle();
    expect(daemon.abortedReads).toBe(1);
    expect(daemon.count('POST', '/api/runs/')).toBe(0);
    expect(daemon.runs.get('r-live')!.info.status).toBe('in_progress');
  });

  it('switching conversation stops reading, and switching back draws it again', async () => {
    daemon.save(2, { role: 'user', content: 'other' });
    daemon.running('r-live', 1, [{ type: 'text_delta', content: 'Hel' }]);
    const hook = await mount(open(1));
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    hook.rerender(open(2));
    await waitFor(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('other'));
    expect(hook.result.current.isRunning).toBe(false);
    await settle();
    expect(daemon.abortedReads).toBe(1);
    expect(daemon.count('POST', '/api/runs/')).toBe(0);

    daemon.emit('r-live', { type: 'text_delta', content: 'lo' });
    hook.rerender(open(1));
    await waitFor(() => expect(shown(hook.result.current.messages).at(-1)?.[2]).toBe('Hello'));
    expect(hook.result.current.isRunning).toBe(true);
  });

  it('Stop cancels the run, and shows the unfinished reply it saved', async () => {
    daemon.running('r-live', 1, [{ type: 'text_delta', content: 'Hel' }]);
    const hook = await mount(open(1));
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    act(() => hook.result.current.runtime.thread.cancelRun());
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(daemon.count('POST', '/api/runs/r-live/cancel')).toBe(1);
    const last = hook.result.current.messages.at(-1)!;
    expect(shown([last])).toEqual([['db-1', 'assistant', 'Hel']]);
    expect(last.status).toEqual({ type: 'incomplete', reason: 'cancelled' });
  });

  it('Stop pressed before the daemon accepted the run cancels it once it has', async () => {
    let release = () => {};
    daemon.startGate = new Promise((r) => {
      release = r;
    });
    const hook = await mount(open(1));
    send(hook, 'hello');
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));

    act(() => hook.result.current.runtime.thread.cancelRun());
    release();

    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(daemon.count('POST', '/api/runs/')).toBe(1);
    expect(daemon.only().info.status).toBe('cancelled');
  });

  it('an edit reaches the database: the old turn is deleted and the new message saved once', async () => {
    daemon.save(1, { role: 'user', content: 'q' });
    daemon.save(1, { role: 'assistant', content: 'a' });
    const hook = await mount(open(1));

    act(() => {
      void hook.result.current.runtime.thread.append({
        parentId: 'system-1',
        role: 'user',
        content: [{ type: 'text', text: 'q2' }],
      });
    });
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const order = daemon.requests.map((r) => `${r.method} ${r.url.split('?')[0]}`);
    expect(order.indexOf('DELETE /api/messages/1')).toBeGreaterThan(-1);
    expect(order.indexOf('DELETE /api/messages/1')).toBeLessThan(order.findIndex((r) => r.startsWith('PUT')));

    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a2' });
    daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a2' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(daemon.saved(1).map((r) => [r.role, r.content])).toEqual([
      ['user', 'q2'],
      ['assistant', 'a2'],
    ]);
    expect(shown(hook.result.current.messages).map(([, role, text]) => [role, text])).toEqual([
      ['system', 'You are a helpful assistant.'],
      ['user', 'q2'],
      ['assistant', 'a2'],
    ]);
  });

  it('regenerate leaves the question saved once, with the new reply', async () => {
    daemon.save(1, { role: 'user', content: 'q' });
    daemon.save(1, { role: 'assistant', content: 'a' });
    const hook = await mount(open(1));

    act(() => {
      void hook.result.current.runtime.thread.startRun({ parentId: 'db-1' });
    });
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));

    expect(daemon.only().request!.messages.at(-1)).toEqual({ role: 'user', content: 'q' });

    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a again' });
    daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a again' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(daemon.saved(1).map((r) => [r.role, r.content])).toEqual([
      ['user', 'q'],
      ['assistant', 'a again'],
    ]);
    expect(shown(hook.result.current.messages).map(([, role, text]) => [role, text])).toEqual([
      ['system', 'You are a helpful assistant.'],
      ['user', 'q'],
      ['assistant', 'a again'],
    ]);
  });
});
