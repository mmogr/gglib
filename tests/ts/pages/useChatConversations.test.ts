/**
 * A conversation's machine is fixed, so the chat page lists only the
 * conversations its session's model can carry on: those that ran on that
 * machine, and those that have not run yet.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import type { ConversationSummary } from '../../../src/services/transport';
import type { Machine } from '../../../src/types/generated/Machine';
import type { ModelRef } from '../../../src/types/generated/ModelRef';

const transport = vi.hoisted(() => ({
  listConversations: vi.fn(async (): Promise<ConversationSummary[]> => []),
  createConversation: vi.fn(async () => 9),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

import { useChatConversations } from '../../../src/pages/useChatConversations';

const desk: Machine = { kind: 'paired', fingerprint: '3ca82708b995' };
const study: Machine = { kind: 'paired', fingerprint: 'ffeeddccbbaa' };

function chat(id: number, model: ModelRef | null, modelId: number | null = null): ConversationSummary {
  return {
    id,
    title: `chat ${id}`,
    model_id: modelId,
    system_prompt: null,
    settings: model ? { model } : null,
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z',
  };
}

const listed = [
  chat(1, { machine: { kind: 'local' }, id: 5 }, 5),
  chat(2, null, 5),
  chat(3, { machine: desk, id: 3 }),
  chat(4, { machine: study, id: 3 }),
  chat(5, null),
];

async function shown(machine?: Machine): Promise<number[]> {
  const { result } = renderHook(() => useChatConversations(1, () => {}, machine));
  await waitFor(() => expect(result.current.conversationLoading).toBe(false));
  return result.current.conversations.map((c) => c.id);
}

describe('useChatConversations, by machine', () => {
  beforeEach(() => {
    transport.listConversations.mockReset();
    transport.listConversations.mockResolvedValue(listed);
  });

  it("a local session lists this machine's chats and those that have not run", async () => {
    expect(await shown()).toEqual([1, 2, 5]);
  });

  it("a paired session lists that machine's chats and those that have not run", async () => {
    expect(await shown(desk)).toEqual([3, 5]);
  });

  it('a paired session opens on one of its own, not the chat it was handed', async () => {
    const { result } = renderHook(() => useChatConversations(1, () => {}, desk));
    await waitFor(() => expect(result.current.conversationLoading).toBe(false));
    expect(result.current.activeConversationId).toBe(3);
  });

  it("a New mark on the other machine's chat is not dropped: the fetched list has it", async () => {
    const { result } = renderHook(() => useChatConversations(null, () => {}, desk));
    await waitFor(() => expect(result.current.fetched).not.toBeNull());
    expect(result.current.fetched?.ids).toEqual([1, 2, 3, 4, 5]);
  });
});
