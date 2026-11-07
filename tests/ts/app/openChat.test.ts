/**
 * "Chat" on a model: its latest conversation there, else an empty chat on
 * it. This machine's list is read first when nothing has read it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const listConversations = vi.hoisted(() => vi.fn(async (): Promise<unknown[]> => []));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => ({ listConversations }),
}));

import { chatPathOn, chatPathOnFar } from '../../../src/app/openChat';
import { resetChats } from '../../../src/services/chatRegistry';

const DESK = { kind: 'paired' as const, fingerprint: '3ca82708b995' };

function chat(id: number, updated_at: string, model_id: number | null, model?: object) {
  return { id, title: `chat ${id}`, model_id, system_prompt: null, created_at: updated_at, updated_at, ...(model && { settings: { model } }) };
}

beforeEach(() => {
  resetChats();
  listConversations.mockReset();
  listConversations.mockResolvedValue([
    chat(1, '2026-11-01 09:00:00', 7),
    chat(2, '2026-11-03 09:00:00', 7),
    chat(3, '2026-11-04 09:00:00', 9),
    chat(4, '2026-11-05 09:00:00', null, { machine: DESK, id: 7 }),
    chat(5, '2026-11-02 09:00:00', null, { machine: DESK, id: 7 }),
    chat(6, '2026-11-06 09:00:00', null, { machine: { kind: 'paired', fingerprint: 'ffeeddccbbaa' }, id: 7 }),
  ]);
});

describe('Chat on a model', () => {
  it("on this machine's model: its newest chat, read first", async () => {
    expect(await chatPathOn(7)).toBe('/chat/2');
    expect(listConversations).toHaveBeenCalledTimes(1);
    // Read once: the store has it now.
    expect(await chatPathOn(9)).toBe('/chat/3');
    expect(listConversations).toHaveBeenCalledTimes(1);
  });

  it('with no chat on it, an empty chat on it', async () => {
    expect(await chatPathOn(11)).toBe('/chat/new?model=11');
  });

  it("on the paired machine's model: the newest chat kept here for that machine's model", async () => {
    expect(await chatPathOnFar({ machine: DESK, id: 7 })).toBe('/chat/4');
    expect(await chatPathOnFar({ machine: DESK, id: 8 })).toBe('/chat/new?far=8');
  });

  it("a list that cannot be read is no chats: an empty chat on the model", async () => {
    listConversations.mockRejectedValue(new Error('daemon gone'));
    expect(await chatPathOn(7)).toBe('/chat/new?model=7');
  });
});
