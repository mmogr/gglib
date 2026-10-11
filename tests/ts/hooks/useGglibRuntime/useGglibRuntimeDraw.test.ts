/**
 * A send with Draw pressed, and the render it starts, as the runtime handles
 * them.
 *
 * A run's body, or a far turn's, says `draw` only when the caller's `draw`
 * gives something at that send, and has no such key otherwise; the caller is
 * told once its turn is accepted, and never for a refused one.
 *
 * A render's preview frames come beside the run's log. They are held beside
 * the messages (`previews`), by tool call, the newest only; no message ever
 * holds one; a call's frame goes when its result is drawn, a late one for it
 * is dropped, and none outlives the run or the reading of it. Its
 * `tool_progress` events are kept on the call's part until its result, and a
 * `waiting` event on the turn until its prompt is read.
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
import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';
import { conversation, mount, send } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';
import type { GglibMessage } from '../../../../src/types/messages';

const local = { conversationId: 1, conversation: conversation(1), selectedServerPort: 9000 };
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAIAAAACA';
const frame = (step: number, b64 = `${PNG}${step}`) => ({ mime: 'image/png', step, total: 20, b64 });

const drawStart = {
  type: 'tool_call_start',
  tool_call: { id: 'call_1', name: 'builtin:generate_image', arguments: { prompt: 'a lighthouse at dusk' } },
  display_name: 'Generate Image',
};
const drawDone = {
  type: 'tool_call_complete',
  tool_name: 'builtin:generate_image',
  result: { tool_call_id: 'call_1', content: 'Drew 1 image.', success: true },
  wait_ms: 0,
  execute_duration_ms: 76000,
  display_name: 'Generate Image',
  duration_display: '76s',
};

/** The tool-call parts of the thread's last message. */
function lastToolCalls(messages: GglibMessage[]) {
  const parts = messages.at(-1)!.content as unknown as Array<Record<string, unknown>>;
  return parts.filter((part) => part.type === 'tool-call');
}

describe('useGglibRuntime, a send with Draw pressed', () => {
  let daemon: FakeDaemon;

  beforeEach(() => {
    daemon = new FakeDaemon();
    vi.stubGlobal('fetch', vi.fn(daemon.fetch));
    resetRemoteState();
  });
  afterEach(() => vi.unstubAllGlobals());

  /** Send `text`, end its run, and give the body the page sent. */
  async function exchange(hook: Awaited<ReturnType<typeof mount>>, text: string) {
    const before = daemon.count('PUT', '/api/runs/');
    send(hook, text);
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(before + 1));
    const run = [...daemon.runs.values()].at(-1)!;
    void daemon.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    return run.request as unknown as Record<string, unknown>;
  }

  it('says draw on the run whose send was armed, and has no such key on any other', async () => {
    let armed = false;
    const draw = vi.fn(() => (armed ? { accepted: () => {} } : undefined));
    const hook = await mount({ ...local, draw });
    expect(draw).not.toHaveBeenCalled();

    expect(Object.keys(await exchange(hook, 'one'))).not.toContain('draw');
    armed = true;
    expect((await exchange(hook, 'two')).draw).toBe(true);
    armed = false;
    expect(Object.keys(await exchange(hook, 'three'))).not.toContain('draw');
    expect(draw).toHaveBeenCalledTimes(3);
  });

  it('a runtime given no draw says nothing of it', async () => {
    const hook = await mount(local);
    expect(Object.keys(await exchange(hook, 'one'))).not.toContain('draw');
  });

  it('calls it accepted once the daemon has taken the run, while its reply is still being written, and not for a run it refused', async () => {
    const onError = vi.fn();
    const accepted = vi.fn();
    const hook = await mount({ ...local, draw: () => ({ accepted }), onError });

    daemon.refuseNext = { status: 400, type: 'drawing_unavailable', error: 'no image model is installed' };
    send(hook, 'draw a lighthouse');
    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(daemon.runs.size).toBe(0);
    expect(accepted).not.toHaveBeenCalled();
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    send(hook, 'draw a lighthouse');
    await waitFor(() => expect(accepted).toHaveBeenCalledTimes(1));
    expect(daemon.only().request).toMatchObject({ draw: true });
    expect(hook.result.current.isRunning).toBe(true);

    void daemon.finish(daemon.only().info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(accepted).toHaveBeenCalledTimes(1);
  });
});

describe('useGglibRuntime, a far turn with Draw pressed', () => {
  let daemons: FakeFarDaemon;

  beforeEach(() => {
    daemons = new FakeFarDaemon();
    vi.stubGlobal('fetch', vi.fn(daemons.fetch));
    resetRemoteState();
  });
  afterEach(() => vi.unstubAllGlobals());

  it('says draw on the turn whose send was armed, tells the caller once it is taken, and no other turn has the key', async () => {
    let armed = false;
    const accepted = vi.fn();
    const hook = await mount({ conversationId: 1, source: 'far', draw: () => (armed ? { accepted } : undefined) });
    const exchange = async (text: string) => {
      const before = daemons.farCount('PUT', '/api/remote/chats/1/turns/');
      send(hook, text);
      await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(before + 1));
      const run = [...daemons.hub.runs.values()].at(-1)!;
      void daemons.hub.finish(run.info.id, 'completed', [{ role: 'assistant', content: 'ok' }]);
      await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
      return daemons.farRequests.filter((r) => r.method === 'PUT').at(-1)!.body;
    };

    expect(await exchange('one')).toEqual({ content: 'one' });
    expect(accepted).not.toHaveBeenCalled();
    armed = true;
    expect(await exchange('two')).toEqual({ content: 'two', draw: true });
    expect(accepted).toHaveBeenCalledTimes(1);
    armed = false;
    const untouched = await exchange('three');
    expect(Object.keys(untouched as object)).toEqual(['content']);
    // Nothing went to this machine's own run route.
    expect(daemons.here.requests).toEqual([]);
  });
});

describe('useGglibRuntime, a render\'s frames and progress', () => {
  let daemon: FakeDaemon;

  beforeEach(() => {
    daemon = new FakeDaemon();
    vi.stubGlobal('fetch', vi.fn(daemon.fetch));
    resetRemoteState();
  });
  afterEach(() => vi.unstubAllGlobals());

  /** A send whose run has started its drawing tool, read by the page. */
  async function drawing() {
    const hook = await mount({ ...local, draw: () => ({ accepted: () => {} }) });
    send(hook, 'draw a lighthouse');
    // The run is accepted and the page is reading it.
    await waitFor(() => expect(daemon.only().streams).toHaveLength(1));
    const id = daemon.only().info.id;
    daemon.emit(id, drawStart);
    await waitFor(() => expect(lastToolCalls(hook.result.current.messages)).toHaveLength(1));
    return { hook, id };
  }

  it('holds a call\'s newest frame beside the messages, and no message ever holds one', async () => {
    const { hook, id } = await drawing();
    expect(hook.result.current.previews.size).toBe(0);

    daemon.preview(id, 'call_1', frame(3));
    await waitFor(() => expect(hook.result.current.previews.get('call_1')).toEqual(frame(3)));
    daemon.preview(id, 'call_1', frame(4));
    await waitFor(() => expect(hook.result.current.previews.get('call_1')).toEqual(frame(4)));
    expect(hook.result.current.previews.size).toBe(1);

    // Not in the messages, by any key, and not in the run's log.
    expect(JSON.stringify(hook.result.current.messages)).not.toContain(PNG);
    expect(JSON.stringify(daemon.only().frames)).not.toContain(PNG);
    expect(daemon.only().info.last_seq).toBe(1);
  });

  it('a frame moves no cursor: the events after it are drawn as the ones before were', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', frame(3));
    daemon.emit(id, { type: 'tool_progress', tool_call_id: 'call_1', stage: 'sampling', pass: 1, done: 3, total: 20 });

    await waitFor(() =>
      expect(lastToolCalls(hook.result.current.messages)[0].progress).toMatchObject({ stage: 'sampling', done: 3, total: 20 }),
    );
    expect(hook.result.current.previews.get('call_1')).toEqual(frame(3));
  });

  it('drops a call\'s frame when its result is drawn, and a late frame for it is not shown', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', frame(20));
    await waitFor(() => expect(hook.result.current.previews.size).toBe(1));

    daemon.emit(id, drawDone);
    await waitFor(() => expect(lastToolCalls(hook.result.current.messages)[0].result).toBe('Drew 1 image.'));
    expect(hook.result.current.previews.size).toBe(0);

    daemon.preview(id, 'call_1', frame(20));
    daemon.emit(id, { type: 'text_delta', content: 'A lighthouse.' });
    await waitFor(() => expect(JSON.stringify(hook.result.current.messages)).toContain('A lighthouse.'));
    expect(hook.result.current.previews.size).toBe(0);
    expect(JSON.stringify(hook.result.current.messages)).not.toContain(PNG);
  });

  it('keeps another call\'s frame when one call ends', async () => {
    const { hook, id } = await drawing();
    daemon.emit(id, { ...drawStart, tool_call: { ...drawStart.tool_call, id: 'call_2' } });
    daemon.preview(id, 'call_1', frame(5));
    daemon.preview(id, 'call_2', frame(2));
    await waitFor(() => expect(hook.result.current.previews.size).toBe(2));

    daemon.emit(id, drawDone);
    await waitFor(() => expect(hook.result.current.previews.size).toBe(1));
    expect(hook.result.current.previews.get('call_2')).toEqual(frame(2));
  });

  it('holds no frame once the run ends, though its tool never finished', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', frame(7));
    await waitFor(() => expect(hook.result.current.previews.size).toBe(1));

    void daemon.finish(id, 'failed');
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(hook.result.current.previews.size).toBe(0);
    expect(JSON.stringify(hook.result.current.messages)).not.toContain(PNG);
  });

  it('holds no frame of a run that is no longer read: another conversation opens with none', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', frame(7));
    await waitFor(() => expect(hook.result.current.previews.size).toBe(1));

    hook.rerender({ conversationId: 2, conversation: conversation(2), selectedServerPort: 9000 });
    await waitFor(() => expect(hook.result.current.previews.size).toBe(0));
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(hook.result.current.previews.size).toBe(0);
  });

  it('holds no frame once the conversation is closed with none opened', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', frame(7));
    await waitFor(() => expect(hook.result.current.previews.size).toBe(1));

    hook.rerender({ selectedServerPort: 9000 });
    await waitFor(() => expect(hook.result.current.previews.size).toBe(0));
    // The run carries on at the daemon; a frame it sends now has no reader.
    expect(daemon.only().streams).toHaveLength(0);
  });

  it('a preview that is not one is not shown', async () => {
    const { hook, id } = await drawing();
    daemon.preview(id, 'call_1', { mime: 'text/html', step: 1, total: 20, b64: PNG });
    daemon.preview(id, 'call_1', { mime: 'image/png', step: 1, total: 20, b64: '' });
    daemon.emit(id, { type: 'tool_progress', tool_call_id: 'call_1', stage: 'loading' });

    await waitFor(() => expect(lastToolCalls(hook.result.current.messages)[0].progress).toMatchObject({ stage: 'loading' }));
    expect(hook.result.current.previews.size).toBe(0);
  });

  it('keeps a tool\'s progress on its call until its result, and the turn\'s wait until its prompt is read', async () => {
    const hook = await mount(local);
    send(hook, 'hello');
    // The run is accepted and the page is reading it.
    await waitFor(() => expect(daemon.only().streams).toHaveLength(1));
    const id = daemon.only().info.id;
    const custom = () => (hook.result.current.messages.at(-1)!.metadata as { custom: Record<string, unknown> }).custom;

    daemon.emit(id, { type: 'waiting', reason: 'image_render', step: 3, total: 20, position: 1 });
    await waitFor(() => expect(custom().waiting).toEqual({ reason: 'image_render', step: 3, total: 20, position: 1 }));
    daemon.emit(id, { type: 'prompt_progress', processed: 10, total: 40, cached: 0, time_ms: 5 });
    await waitFor(() => expect(custom().prompt).toBeDefined());
    expect(custom().waiting).toBeUndefined();

    daemon.emit(id, drawStart);
    daemon.emit(id, { type: 'tool_progress', tool_call_id: 'call_1', stage: 'queued', position: 2 });
    await waitFor(() => expect(lastToolCalls(hook.result.current.messages)[0].progress).toMatchObject({ stage: 'queued', position: 2 }));
    daemon.emit(id, drawDone);
    await waitFor(() => expect(lastToolCalls(hook.result.current.messages)[0].result).toBe('Drew 1 image.'));
    expect(lastToolCalls(hook.result.current.messages)[0].progress).toBeUndefined();
  });
});
