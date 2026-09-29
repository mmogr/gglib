/**
 * Running and New: which conversations have a reply going, and which have
 * one that ended while another conversation was on screen.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { RunInfo } from '../../../src/types/generated/RunInfo';

const runs = vi.hoisted(() => ({ current: [] as RunInfo[] }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => ({ listRuns: async () => runs.current }) };
});

import {
  UNREAD_STORAGE_KEY,
  useConversationActivity,
} from '../../../src/components/ConversationListPanel/useConversationActivity';

const POLL = 10;

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

function stored(): Record<string, number> {
  return JSON.parse(window.localStorage.getItem(UNREAD_STORAGE_KEY) ?? '{}');
}

/** Let `n` polls go by. */
async function polls(n = 3) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, POLL * n));
  });
}

beforeEach(() => {
  window.localStorage.clear();
  runs.current = [];
});

describe('useConversationActivity', () => {
  it('marks a conversation Running while a run is live in it', async () => {
    runs.current = [run('a', 2, true)];
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    await waitFor(() => expect([...result.current.running]).toEqual([2]));

    runs.current = [run('a', 2, false, Date.now())];
    await waitFor(() => expect(result.current.running.size).toBe(0));
  });

  it('marks New a reply that ended while another conversation was shown, and keeps only its id and time', async () => {
    runs.current = [run('a', 2, true)];
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    await polls();

    runs.current = [run('a', 2, false, 5_000_000_000_000)];
    await waitFor(() => expect([...result.current.unread]).toEqual([2]));
    expect(stored()).toEqual({ 2: 5_000_000_000_000 });
  });

  it('does not mark the conversation on screen', async () => {
    runs.current = [run('a', 1, true)];
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    await polls();

    runs.current = [run('a', 1, false, Date.now())];
    await waitFor(() => expect(result.current.running.size).toBe(0));
    await polls();
    expect(result.current.unread.size).toBe(0);
    expect(stored()).toEqual({});
  });

  it('clears the mark when the conversation is shown', async () => {
    runs.current = [run('a', 2, true)];
    const { result, rerender } = renderHook(({ active }) => useConversationActivity(active, null, POLL), {
      initialProps: { active: 1 as number | null },
    });
    await polls();
    runs.current = [run('a', 2, false, Date.now() + 60_000)];
    await waitFor(() => expect(result.current.unread.has(2)).toBe(true));

    rerender({ active: 2 });
    expect(result.current.unread.size).toBe(0);
    expect(stored()).toEqual({});
    await polls();
    expect(result.current.unread.size).toBe(0);
  });

  it('marks nothing for runs that had ended before the page first asked', async () => {
    runs.current = [run('old', 2, false, 1_000)];
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    await polls();
    expect(result.current.unread.size).toBe(0);
  });

  it('does not mark a reply that ended while its conversation was still on screen', async () => {
    runs.current = [run('a', 2, true)];
    const { result, rerender } = renderHook(({ active }) => useConversationActivity(active, null, POLL), {
      initialProps: { active: 2 as number | null },
    });
    await polls();
    const endedWhileShown = Date.now();
    await new Promise((resolve) => setTimeout(resolve, 5));
    rerender({ active: 1 });
    runs.current = [run('a', 2, false, endedWhileShown)];
    await polls();
    expect(result.current.unread.size).toBe(0);
  });

  it('remembers marks across a reload', () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 3: 42, junk: 'x' }));
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    expect([...result.current.unread]).toEqual([3]);
  });

  it('takes a mark another tab cleared as cleared, and never writes it back', async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 7 }));
    runs.current = [run('b', 3, true)];
    const { result } = renderHook(() => useConversationActivity(1, null, POLL));
    expect([...result.current.unread]).toEqual([2]);
    await polls();

    // The other tab shows conversation 2.
    act(() => {
      window.localStorage.setItem(UNREAD_STORAGE_KEY, '{}');
      window.dispatchEvent(new StorageEvent('storage', { key: UNREAD_STORAGE_KEY }));
    });
    expect(result.current.unread.size).toBe(0);

    // This tab then marks another: 2 stays cleared.
    runs.current = [run('b', 3, false, 5_000_000_000_000)];
    await waitFor(() => expect([...result.current.unread]).toEqual([3]));
    expect(stored()).toEqual({ 3: 5_000_000_000_000 });
  });

  it('reads what is stored before writing, even when no event said it changed', async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 7 }));
    const { rerender } = renderHook(({ active }) => useConversationActivity(active, null, POLL), {
      initialProps: { active: 1 as number | null },
    });
    // Another tab clears 2 and marks 4; this tab heard nothing of it, then
    // shows 2 itself: 4 stays marked.
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 4: 9 }));
    rerender({ active: 2 });
    expect(stored()).toEqual({ 4: 9 });
  });

  it('drops the marks of conversations no longer listed, once the list has loaded', () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 2: 7, 99: 8 }));
    const { result, rerender } = renderHook(({ listed }) => useConversationActivity(1, listed, POLL), {
      initialProps: { listed: null as number[] | null },
    });
    expect(stored()).toEqual({ 2: 7, 99: 8 });
    rerender({ listed: [] });
    expect(stored()).toEqual({ 2: 7, 99: 8 });

    rerender({ listed: [1, 2] });
    expect(stored()).toEqual({ 2: 7 });
    expect([...result.current.unread]).toEqual([2]);
  });
});
