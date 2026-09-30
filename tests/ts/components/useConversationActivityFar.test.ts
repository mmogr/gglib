/**
 * Running and New for the far machine's chats: its runs are asked of the far
 * machine, and its marks kept apart from this machine's, so an id there is
 * never read as one here and switching keeps this machine's marks.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { RunInfo } from '../../../src/types/generated/RunInfo';
import type { ChatSource } from '../../../src/services/transport';

const runs = vi.hoisted(() => ({ here: [] as RunInfo[], far: [] as RunInfo[] }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({ listRuns: async () => runs.here, listFarRuns: async () => runs.far }),
  };
});

import {
  UNREAD_STORAGE_KEY,
  unreadStorageKey,
  useConversationActivity,
  type FetchedList,
} from '../../../src/components/ConversationListPanel/useConversationActivity';

const POLL = 10;
const FAR_KEY = 'gglib.chat.unread.far';

function run(id: string, conversationId: number, live: boolean, finishedAt = 1_000): RunInfo {
  return {
    id,
    kind: 'agent',
    status: live ? 'in_progress' : 'completed',
    created_at_ms: 1,
    conversation_id: conversationId,
    last_seq: 3,
    ...(!live && { finished_at_ms: finishedAt }),
  };
}

function stored(key: string): Record<string, number> {
  return JSON.parse(window.localStorage.getItem(key) ?? '{}');
}

async function polls(n = 3) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, POLL * n));
  });
}

type Props = { active: number | null; fetched: FetchedList | null; source: ChatSource };

function mount(initial: Props) {
  return renderHook((p: Props) => useConversationActivity(p.active, p.fetched, POLL, p.source), {
    initialProps: initial,
  });
}

beforeEach(() => {
  window.localStorage.clear();
  runs.here = [];
  runs.far = [];
});

describe('useConversationActivity, by machine', () => {
  it('keeps each machine’s marks under its own key', () => {
    expect(unreadStorageKey('this')).toBe(UNREAD_STORAGE_KEY);
    expect(unreadStorageKey('far')).toBe(FAR_KEY);
  });

  it('marks a far reply New under the far key, and leaves this machine’s alone', async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 5 }));
    runs.far = [run('f', 2, true)];
    runs.here = [run('h', 3, true)];
    const hook = mount({ active: 1, fetched: null, source: 'far' });
    await waitFor(() => expect([...hook.result.current.running]).toEqual([2]));
    expect(hook.result.current.unread.size).toBe(0);

    runs.far = [run('f', 2, false, Date.now())];
    await waitFor(() => expect([...hook.result.current.unread]).toEqual([2]));
    expect(Object.keys(stored(FAR_KEY))).toEqual(['2']);
    expect(stored(UNREAD_STORAGE_KEY)).toEqual({ 2: 5 });
  });

  it('switching machines shows each one’s marks, and keeps this machine’s', async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 5 }));
    window.localStorage.setItem(FAR_KEY, JSON.stringify({ 9: 7 }));
    const hook = mount({ active: 1, fetched: null, source: 'this' });
    expect([...hook.result.current.unread]).toEqual([2]);

    hook.rerender({ active: null, fetched: null, source: 'far' });
    await waitFor(() => expect([...hook.result.current.unread]).toEqual([9]));

    hook.rerender({ active: null, fetched: null, source: 'this' });
    await waitFor(() => expect([...hook.result.current.unread]).toEqual([2]));
    await polls();
    expect(stored(UNREAD_STORAGE_KEY)).toEqual({ 2: 5 });
    expect(stored(FAR_KEY)).toEqual({ 9: 7 });
  });

  it('a far listing drops no mark of this machine’s', async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 5 }));
    const hook = mount({ active: 1, fetched: null, source: 'this' });

    hook.rerender({ active: 1, fetched: { ids: [9], askedAt: 10_000, source: 'far' }, source: 'this' });
    await polls();

    expect([...hook.result.current.unread]).toEqual([2]);
    expect(stored(UNREAD_STORAGE_KEY)).toEqual({ 2: 5 });
  });
});
