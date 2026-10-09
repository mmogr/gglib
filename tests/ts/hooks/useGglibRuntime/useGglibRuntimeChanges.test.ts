/**
 * An edit, a regenerate, Retry and Branch from here, as the page makes
 * them (ADR 0017), and the options a chat's family holds along it. A
 * change is the daemon's to make: one that would rewrite a saved reply is
 * made on a new branch, which the page opens and answers there, and the
 * chat it was made on is left as it was. Only an edit of the last question,
 * while nothing answers it, is made in place. The page deletes nothing, and
 * a refused change changes nothing.
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

/** What `cid` holds, as saved. */
const saved = (cid: number) => daemon.saved(cid).map((r) => [r.role, r.content]);

/** What the thread shows, as role and text. */
const seen = (hook: Awaited<ReturnType<typeof mount>>) =>
  shown(hook.result.current.messages).map(([, role, text]) => [role, text]);

/** The change the page posted, its only one. */
function posted() {
  const changes = daemon.requests.filter((r) => r.method === 'POST' && r.url.endsWith('/changes'));
  expect(changes).toHaveLength(1);
  return { url: changes[0].url, body: changes[0].body };
}

describe('useGglibRuntime changes', () => {
  it('an edit of an answered question opens a branch, answered there, and leaves the chat as it was', async () => {
    const onConversationChanged = vi.fn();
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onConversationChanged, onBranched });
    edit(hook, 'system-1', 'q2');

    await waitFor(() => expect(onBranched).toHaveBeenCalled());
    expect(posted()).toEqual({
      url: '/api/conversations/1/changes',
      body: { kind: 'edit', message_id: 1, content: 'q2' },
    });
    expect(onBranched).toHaveBeenCalledWith(100, undefined);
    expect(onConversationChanged).toHaveBeenCalledWith(100);
    expect(daemon.only().request).toMatchObject({ conversation_id: 100, answer_saved: true, messages: [] });
    expect(saved(1)).toEqual([['user', 'q'], ['assistant', 'a']]);
    expect(saved(100)).toEqual([['user', 'q2']]);
    expect(hook.result.current.isRunning).toBe(false);
    expect(daemon.count('DELETE', '/api/messages')).toBe(0);
  });

  it('an edit of the last question nothing answers is made in place, then answered and read', async () => {
    daemon.save(1, { role: 'user', content: 'q3' });
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onBranched });
    expect(hook.result.current.branching.answerable).toBe(true);
    edit(hook, 'db-2', 'q4');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ conversation_id: 1, answer_saved: true, messages: [] });
    // The edited question is shown as saved while its reply is read.
    await waitFor(() => expect(seen(hook)).toContainEqual(['user', 'q4']));
    expect(seen(hook)).not.toContainEqual(['user', 'q3']);
    const { id } = daemon.only().info;
    daemon.emit(id, { type: 'final_answer', content: 'a4' });
    void daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a4' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    expect(saved(1)).toEqual([['user', 'q'], ['assistant', 'a'], ['user', 'q4'], ['assistant', 'a4']]);
    expect(seen(hook).slice(1)).toEqual(saved(1));
    expect(hook.result.current.branching.answerable).toBe(false);
    expect(onBranched).not.toHaveBeenCalled();
  });

  it('a regenerate opens a branch holding the question, answered there', async () => {
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onBranched });
    regenerate(hook, 'db-1');

    await waitFor(() => expect(onBranched).toHaveBeenCalledWith(100, undefined));
    expect(posted().body).toEqual({ kind: 'regenerate', message_id: 2 });
    expect(daemon.only().request).toMatchObject({ conversation_id: 100, answer_saved: true });
    expect(saved(100)).toEqual([['user', 'q']]);
    expect(saved(1)).toEqual([['user', 'q'], ['assistant', 'a']]);
  });

  it('an edit of a reply opens a branch holding it as written, and asks no answer', async () => {
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onBranched });
    edit(hook, 'db-1', 'a, put better', { sourceId: 'db-2' });

    await waitFor(() => expect(onBranched).toHaveBeenCalledWith(100, undefined));
    expect(posted().body).toEqual({ kind: 'edit', message_id: 2, content: 'a, put better' });
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    expect(saved(100)).toEqual([['user', 'q'], ['assistant', 'a, put better']]);
    expect(saved(1)).toEqual([['user', 'q'], ['assistant', 'a']]);
  });

  it('Branch from here copies the chat to the end of the turn into a branch, opened and not answered', async () => {
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onBranched });
    act(() => void hook.result.current.branching.branchFrom('db-1'));

    await waitFor(() => expect(onBranched).toHaveBeenCalledWith(100, undefined));
    expect(posted().body).toEqual({ kind: 'branch', message_id: 1 });
    expect(saved(100)).toEqual([['user', 'q']]);
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    expect(saved(1)).toEqual([['user', 'q'], ['assistant', 'a']]);
  });

  it('a chat reads the options its family holds where its branches part, and opens one through the page', async () => {
    const first = await mount(open);
    regenerate(first, 'db-1');
    await waitFor(() => expect(daemon.conversations.has(100)).toBe(true));
    void daemon.finish(daemon.only().info.id, 'completed', [{ role: 'assistant', content: 'a again' }]);
    await waitFor(() => expect(saved(100)).toHaveLength(2));
    first.unmount();

    const onConversationChanged = vi.fn();
    const hook = await mount({ ...open, onConversationChanged });
    expect(hook.result.current.branching.points).toEqual([
      {
        message_id: 2,
        index: 0,
        options: [
          { conversation_id: 1, message_id: 2, role: 'assistant', preview: 'a' },
          { conversation_id: 100, message_id: 4, role: 'assistant', preview: 'a again' },
        ],
      },
    ]);

    const before = hook.result.current.branching;
    hook.rerender({ ...open, onConversationChanged });
    expect(hook.result.current.branching).toBe(before);

    hook.result.current.branching.open(100);
    expect(onConversationChanged).toHaveBeenCalledWith(100);
  });

  it('Retry answers the question the chat ends in, and is offered no more once it is', async () => {
    daemon.save(1, { role: 'user', content: 'q3' });
    const hook = await mount(open);
    act(() => void hook.result.current.branching.retry());

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.requests.some((r) => r.url.endsWith('/changes'))).toBe(false);
    expect(daemon.only().request).toMatchObject({ conversation_id: 1, answer_saved: true, messages: [] });
    const { id } = daemon.only().info;
    void daemon.finish(id, 'completed', [{ role: 'assistant', content: 'a3' }]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));
    expect(hook.result.current.branching.answerable).toBe(false);
    expect(saved(1).at(-1)).toEqual(['assistant', 'a3']);
  });

  it('an answer run says the Thinking choice as a send does', async () => {
    daemon.save(1, { role: 'user', content: 'q3' });
    const accepted = vi.fn();
    const hook = await mount({ ...open, thinking: () => ({ said: 'off', accepted }) });
    edit(hook, 'db-2', 'q4');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(daemon.only().request).toMatchObject({ answer_saved: true, thinking: 'off' });
    await waitFor(() => expect(accepted).toHaveBeenCalledTimes(1));
  });

  it('a branch whose answer is refused is still opened, and says why it was not answered', async () => {
    const onBranched = vi.fn();
    const onError = vi.fn();
    const hook = await mount({ ...open, onBranched, onError });
    daemon.refuseNext = { status: 429, type: 'agent_busy', error: 'all agent loop slots are in use; try again later' };
    edit(hook, 'system-1', 'q2');

    await waitFor(() => expect(onBranched).toHaveBeenCalled());
    expect(onBranched.mock.calls[0][0]).toBe(100);
    expect(onBranched.mock.calls[0][1].message).toBe('all agent loop slots are in use; try again later');
    expect(onError).not.toHaveBeenCalled();
    expect(saved(100)).toEqual([['user', 'q2']]);
    expect(hook.result.current.isRunning).toBe(false);
  });

  it('an answer refused in place leaves the change made, and Retry still offered', async () => {
    daemon.save(1, { role: 'user', content: 'q3' });
    const onError = vi.fn();
    const hook = await mount({ ...open, onError });
    daemon.refuseNext = { status: 503, type: 'unavailable', error: 'the model is not ready' };
    edit(hook, 'db-2', 'q4');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('the model is not ready');
    expect(saved(1).at(-1)).toEqual(['user', 'q4']);
    await waitFor(() => expect(seen(hook).at(-1)).toEqual(['user', 'q4']));
    expect(hook.result.current.branching.answerable).toBe(true);
    expect(hook.result.current.isRunning).toBe(false);
  });

  it('a change that does not reach the daemon changes nothing, and says so', async () => {
    const onError = vi.fn();
    const onBranched = vi.fn();
    const hook = await mount({ ...open, onError, onBranched });
    daemon.before = (method, path) => {
      if (method === 'POST' && path.endsWith('/changes')) throw new TypeError('Failed to fetch');
    };
    regenerate(hook, 'db-1');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('Failed to fetch');
    expect(onBranched).not.toHaveBeenCalled();
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    expect(daemon.conversations.has(100)).toBe(false);
    expect(seen(hook).slice(1)).toEqual([['user', 'q'], ['assistant', 'a']]);
    expect(hook.result.current.isRunning).toBe(false);
  });

  it('makes no change while a reply is being read', async () => {
    daemon.running('live', 1);
    const hook = await mount(open);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(true));
    regenerate(hook, 'db-1');
    act(() => void hook.result.current.branching.retry());

    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(daemon.requests.some((r) => r.url.endsWith('/changes'))).toBe(false);
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    void daemon.finish('live', 'completed');
  });

  it('asks nothing of a run for a chat with no server to answer it', async () => {
    const onError = vi.fn();
    const hook = await mount({ ...open, selectedServerPort: undefined, onError });
    regenerate(hook, 'db-1');

    await waitFor(() => expect(onError).toHaveBeenCalled());
    expect(onError.mock.calls[0][0].message).toBe('No server selected. Please serve a model first.');
    expect(daemon.requests.some((r) => r.url.endsWith('/changes'))).toBe(false);
  });
});
