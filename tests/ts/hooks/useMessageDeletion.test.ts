/**
 * Tests for useMessageDeletion.
 *
 * The delete flow reloads the whole thread from the database rather than
 * patching it in place, so the reload has to reconstruct messages exactly the
 * way initial hydration does. It previously did not: tool rows were left
 * unfolded and content parts were dropped, so a mid-thread delete silently
 * stripped tool blocks off the surviving assistant messages until the next
 * hydrate. These tests pin that down.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { createRef } from 'react';
import type { ThreadMessageLike } from '@assistant-ui/react';

const transport = vi.hoisted(() => ({
  getMessages: vi.fn(),
  deleteMessage: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useMessageDeletion } from '../../../src/components/ChatMessagesPanel/hooks/useMessageDeletion';
import type { ChatMessage, ConversationSummary } from '../../../src/services/transport';

const CONVERSATION_ID = 7;

const conversation: ConversationSummary = {
  id: CONVERSATION_ID,
  title: 'Test',
  model_id: null,
  system_prompt: null,
  settings: null,
  created_at: '2024-01-01T00:00:00Z',
  updated_at: '2024-01-01T00:00:00Z',
};

/** An assistant row with a CLI tool call, plus its separate tool-result row. */
const messagesWithToolCall: ChatMessage[] = [
  {
    id: 1,
    conversation_id: CONVERSATION_ID,
    role: 'user',
    content: 'what time is it?',
    created_at: '2024-01-01T00:00:01Z',
  },
  {
    id: 2,
    conversation_id: CONVERSATION_ID,
    role: 'assistant',
    content: 'let me check',
    created_at: '2024-01-01T00:00:02Z',
    metadata: {
      tool_calls: [{ id: 'call-1', name: 'get_current_time', arguments: { tz: 'UTC' } }],
    },
  },
  {
    id: 3,
    conversation_id: CONVERSATION_ID,
    role: 'tool',
    content: '12:00',
    created_at: '2024-01-01T00:00:03Z',
    metadata: { tool_call_id: 'call-1' },
  },
];

function setup(dbMessages: ChatMessage[], runtimeMessages: Array<{ id: string; role: string }>) {
  const reset = vi.fn();
  const threadRuntime = {
    getState: () => ({ messages: runtimeMessages }),
    reset,
  } as any;

  const persistedMessageIds = createRef<Set<string>>() as React.MutableRefObject<Set<string>>;
  persistedMessageIds.current = new Set();
  const dbIdByPosition = createRef<Map<number, number>>() as React.MutableRefObject<Map<number, number>>;
  dbIdByPosition.current = new Map();

  const syncConversations = vi.fn().mockResolvedValue(undefined);
  const showToast = vi.fn();

  transport.getMessages.mockResolvedValue(dbMessages);
  transport.deleteMessage.mockResolvedValue({ deletedCount: 1 });

  const hook = renderHook(() =>
    useMessageDeletion({
      threadRuntime,
      activeConversationId: CONVERSATION_ID,
      activeConversation: conversation,
      persistedMessageIds,
      dbIdByPosition,
      syncConversations,
      showToast,
    })
  );

  return { hook, reset, persistedMessageIds, dbIdByPosition, syncConversations, showToast };
}

describe('useMessageDeletion', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('opens and closes the confirmation modal', async () => {
    const { hook } = setup([], []);

    expect(hook.result.current.isDeleteModalOpen).toBe(false);

    act(() => hook.result.current.initiateDelete('db-4'));
    await waitFor(() => expect(hook.result.current.isDeleteModalOpen).toBe(true));

    act(() => hook.result.current.cancelDelete());
    await waitFor(() => expect(hook.result.current.isDeleteModalOpen).toBe(false));
  });

  it('reports the cascade count for the pending message', async () => {
    const { hook } = setup([], [
      { id: 'system-7', role: 'system' },
      { id: 'db-1', role: 'user' },
      { id: 'db-2', role: 'assistant' },
      { id: 'db-4', role: 'user' },
    ]);

    act(() => hook.result.current.initiateDelete('db-2'));

    // The target plus every following non-system message.
    await waitFor(() => expect(hook.result.current.pendingDeleteCount).toBe(2));
  });

  it('deletes by the DB id encoded in the runtime message id', async () => {
    const { hook } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);

    act(() => hook.result.current.initiateDelete('db-9'));
    await act(async () => { await hook.result.current.confirmDelete(); });

    expect(transport.deleteMessage).toHaveBeenCalledWith(9);
  });

  it('falls back to the position map for messages created this session', async () => {
    const { hook, dbIdByPosition } = setup(messagesWithToolCall, [
      { id: 'temp-abc', role: 'user' },
    ]);
    dbIdByPosition.current.set(0, 42);

    act(() => hook.result.current.initiateDelete('temp-abc'));
    await act(async () => { await hook.result.current.confirmDelete(); });

    expect(transport.deleteMessage).toHaveBeenCalledWith(42);
  });

  it('preserves tool-call content parts on the reloaded thread', async () => {
    const { hook, reset } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);

    act(() => hook.result.current.initiateDelete('db-9'));
    await act(async () => { await hook.result.current.confirmDelete(); });

    expect(reset).toHaveBeenCalledTimes(1);
    const reloaded = reset.mock.calls[0][0] as ThreadMessageLike[];

    // The standalone tool row is folded away, not rendered as its own message.
    expect(reloaded).toHaveLength(2);
    expect(reloaded.map((m) => m.role)).toEqual(['user', 'assistant']);

    // The assistant keeps its tool call, with the result merged in.
    const assistant = reloaded[1];
    const parts = assistant.content as Array<Record<string, unknown>>;
    const toolPart = parts.find((p) => p.type === 'tool-call');
    expect(toolPart).toMatchObject({
      toolCallId: 'call-1',
      toolName: 'get_current_time',
      args: { tz: 'UTC' },
      result: '12:00',
    });
  });

  it('rebuilds the position map and persisted ids from the reloaded rows', async () => {
    const { hook, dbIdByPosition, persistedMessageIds } = setup(messagesWithToolCall, [
      { id: 'db-9', role: 'user' },
    ]);

    act(() => hook.result.current.initiateDelete('db-9'));
    await act(async () => { await hook.result.current.confirmDelete(); });

    // No system prompt on this conversation, so positions start at 0.
    expect(dbIdByPosition.current.get(0)).toBe(1);
    expect(dbIdByPosition.current.get(1)).toBe(2);
    expect(persistedMessageIds.current).toEqual(new Set(['db-1', 'db-2']));
  });

  it('reports failure without leaving the modal open', async () => {
    const { hook, showToast } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);
    transport.deleteMessage.mockRejectedValueOnce(new Error('boom'));

    act(() => hook.result.current.initiateDelete('db-9'));
    await act(async () => { await hook.result.current.confirmDelete(); });

    expect(showToast).toHaveBeenCalledWith('Failed to delete message', 'error');
    expect(hook.result.current.isDeleteModalOpen).toBe(false);
    expect(hook.result.current.isDeleting).toBe(false);
  });
});
