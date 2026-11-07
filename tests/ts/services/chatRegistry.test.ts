/**
 * The chat list store: a read that lands after a newer one was asked is
 * dropped; the list remembers when it was asked for, before the read, so a
 * New mark made while a read is out is not taken as dropped by it; a read
 * that fails keeps the list it had and says so, and a first that fails is
 * empty and failed, not "no chats".
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const listConversations = vi.hoisted(() => vi.fn<() => Promise<unknown[]>>());
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => ({ listConversations }),
}));

import { refreshChats, resetChats, useChats } from '../../../src/services/chatRegistry';
import { renderHook, act } from '@testing-library/react';

function chat(id: number, updated_at = '2026-11-01 09:00:00') {
  return { id, title: `chat ${id}`, model_id: null, system_prompt: null, created_at: updated_at, updated_at };
}

/** A read held until the test answers it. */
function held() {
  let answer: (list: unknown[]) => void = () => {};
  let fail: (error: Error) => void = () => {};
  listConversations.mockImplementationOnce(
    () =>
      new Promise((resolve, reject) => {
        answer = resolve;
        fail = reject;
      }),
  );
  return { answer: (list: unknown[]) => answer(list), fail: (error: Error) => fail(error) };
}

const state = () => renderHook(() => useChats('this')).result;

beforeEach(() => {
  resetChats();
  listConversations.mockReset();
});

describe('chatRegistry', () => {
  it('drops a read that lands after a newer one was asked', async () => {
    const view = state();
    const older = held();
    const first = refreshChats();
    const newer = held();
    const second = refreshChats();

    await act(async () => {
      newer.answer([chat(2)]);
      await second;
    });
    await act(async () => {
      older.answer([chat(1)]);
      await first;
    });

    expect(view.current.list?.map((c) => c.id)).toEqual([2]);
  });

  it('remembers when the list was asked for, before the read, not when it landed', async () => {
    const view = state();
    const read = held();
    const before = Date.now();
    const out = refreshChats();
    // A New mark made now, while the read is out, postdates the asking.
    await new Promise((resolve) => setTimeout(resolve, 20));
    const markedAt = Date.now();
    await act(async () => {
      read.answer([chat(1)]);
      await out;
    });

    const askedAt = view.current.fetched!.askedAt;
    expect(askedAt).toBeGreaterThanOrEqual(before);
    expect(askedAt).toBeLessThan(markedAt);
    expect(view.current.fetched!.ids).toEqual([1]);
  });

  it('keeps the list it had when a read fails, and says the read failed until one lands', async () => {
    const view = state();
    listConversations.mockResolvedValueOnce([chat(1)]);
    await act(() => refreshChats());
    expect(view.current.failed).toBe(false);

    listConversations.mockRejectedValueOnce(new Error('daemon gone'));
    await act(() => refreshChats());
    expect(view.current.list?.map((c) => c.id)).toEqual([1]);
    expect(view.current.failed).toBe(true);

    listConversations.mockResolvedValueOnce([chat(1), chat(3)]);
    await act(() => refreshChats());
    expect(view.current.failed).toBe(false);
  });

  it('a first read that fails is empty and failed, not "no chats"', async () => {
    const view = state();
    listConversations.mockRejectedValueOnce(new Error('daemon gone'));
    await act(() => refreshChats());
    expect(view.current.list).toEqual([]);
    expect(view.current.failed).toBe(true);
  });

  it('lists newest first', async () => {
    const view = state();
    listConversations.mockResolvedValueOnce([chat(1, '2026-11-01 09:00:00'), chat(2, '2026-11-03 09:00:00'), chat(3, '2026-11-02 09:00:00')]);
    await act(() => refreshChats());
    expect(view.current.list?.map((c) => c.id)).toEqual([2, 3, 1]);
  });
});
