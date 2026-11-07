/**
 * Tests for useMessageDeletion.
 *
 * The delete flow reloads the whole thread from the database rather than
 * patching it in place, so the reload has to reconstruct messages exactly the
 * way initial hydration does. It previously did not: tool rows were left
 * unfolded and content parts were dropped, so a mid-thread delete silently
 * stripped tool blocks off the surviving assistant messages until the next
 * hydrate. These tests pin that down.
 *
 * Delete… asks first through the one confirm sheet, saying how many
 * messages after it go too; a no deletes nothing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { ThreadMessageLike } from '@assistant-ui/react';

const transport = vi.hoisted(() => ({
  getThread: vi.fn(),
  deleteMessage: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { deleteQuestion, useMessageDeletion } from '../../../src/components/ChatMessagesPanel/hooks/useMessageDeletion';
import { TransportError } from '../../../src/services/transport';
import type { ChatMessage, ConversationSummary } from '../../../src/services/transport';

const CONVERSATION_ID = 7;

const conversation: ConversationSummary = {
  id: CONVERSATION_ID,
  title: 'Test',
  model_id: null,
  system_prompt: null,
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

function setup(dbMessages: ChatMessage[], runtimeMessages: Array<{ id: string; role: string }>, answer = true) {
  const reset = vi.fn();
  const threadRuntime = {
    getState: () => ({ messages: runtimeMessages }),
    reset,
  } as any;

  const syncConversations = vi.fn().mockResolvedValue(undefined);
  const showToast = vi.fn();
  const confirm = vi.fn(async () => answer);

  transport.getThread.mockResolvedValue({ messages: dbMessages });
  transport.deleteMessage.mockResolvedValue(1);

  const hook = renderHook(() =>
    useMessageDeletion({
      threadRuntime,
      activeConversationId: CONVERSATION_ID,
      activeConversation: conversation,
      syncConversations,
      showToast,
      confirm,
    })
  );

  /** Delete `id`, as Delete… does, the sheet answered `answer`. */
  const remove = (id: string) => act(async () => { await hook.result.current(id); });

  return { hook, reset, syncConversations, showToast, confirm, remove };
}

describe('useMessageDeletion', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('asks first, through the confirm sheet, and deletes nothing on a no', async () => {
    const { confirm, remove, reset } = setup(messagesWithToolCall, [{ id: 'db-4', role: 'user' }], false);

    await remove('db-4');

    expect(confirm).toHaveBeenCalledWith(deleteQuestion(1));
    expect(transport.deleteMessage).not.toHaveBeenCalled();
    expect(reset).not.toHaveBeenCalled();
  });

  it('says how many messages after it go too: every following non-system message', async () => {
    const { confirm, remove } = setup([], [
      { id: 'system-7', role: 'system' },
      { id: 'db-1', role: 'user' },
      { id: 'db-2', role: 'assistant' },
      { id: 'db-4', role: 'user' },
    ], false);

    await remove('db-2');

    expect(confirm).toHaveBeenCalledWith(deleteQuestion(2));
  });

  it('words the question for none, one and several messages after it', () => {
    expect(deleteQuestion(1)).toEqual({
      title: 'Delete this message?',
      description: 'This cannot be undone.',
      confirmLabel: 'Delete',
      variant: 'danger',
    });
    expect(deleteQuestion(2).description).toBe('The 1 message after it is deleted too. This cannot be undone.');
    expect(deleteQuestion(4).description).toBe('The 3 messages after it are deleted too. This cannot be undone.');
  });

  it('deletes by the DB id encoded in the runtime message id', async () => {
    const { remove } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);

    await remove('db-9');

    expect(transport.deleteMessage).toHaveBeenCalledWith(9);
  });

  it('deletes nothing for a message that is not saved yet, and still reloads', async () => {
    // A reply being drawn has no row until its run ends; every saved row
    // carries its id in its runtime id.
    const { remove, reset } = setup(messagesWithToolCall, [{ id: 'temp-abc', role: 'user' }]);

    await remove('temp-abc');

    expect(transport.deleteMessage).not.toHaveBeenCalled();
    expect(reset).toHaveBeenCalledTimes(1);
  });

  it('preserves tool-call content parts on the reloaded thread', async () => {
    const { remove, reset } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);

    await remove('db-9');

    expect(reset).toHaveBeenCalledTimes(1);
    const reloaded = reset.mock.calls[0][0] as ThreadMessageLike[];

    // The standalone tool row is folded away, not rendered as its own message.
    expect(reloaded).toHaveLength(2);
    expect(reloaded.map((m) => m.role)).toEqual(['user', 'assistant']);

    // The assistant keeps its tool call, with the result merged in.
    const assistant = reloaded[1];
    const parts = assistant.content as unknown as Array<Record<string, unknown>>;
    const toolPart = parts.find((p) => p.type === 'tool-call');
    expect(toolPart).toMatchObject({
      toolCallId: 'call-1',
      toolName: 'get_current_time',
      args: { tz: 'UTC' },
      result: '12:00',
    });
  });

  it('reports a failure', async () => {
    const { remove, showToast } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);
    transport.deleteMessage.mockRejectedValueOnce(new Error('boom'));

    await remove('db-9');

    expect(showToast).toHaveBeenCalledWith('Failed to delete message', 'error');
  });

  it('says a chat whose reply is still being written is not deleted from', async () => {
    const { remove, showToast } = setup(messagesWithToolCall, [{ id: 'db-9', role: 'user' }]);
    transport.deleteMessage.mockRejectedValueOnce(
      new TransportError('CONFLICT', 'conversation 7 already has a live reply, run e1; stop it or wait'),
    );

    await remove('db-9');

    expect(showToast).toHaveBeenCalledWith(
      'A reply is still being written here. Stop it or wait, then delete.',
      'error',
    );
  });
});
