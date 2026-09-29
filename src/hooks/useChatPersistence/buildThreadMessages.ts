import type { ThreadMessageLike } from '@assistant-ui/react';
import type { ChatMessage, ConversationSummary } from '../../services/transport';
import { buildLoadedMessage, foldToolMessages } from './buildLoadedMessage';

/** What of a conversation its thread shows besides its rows. */
export type ThreadConversation = Pick<ConversationSummary, 'id' | 'system_prompt' | 'created_at'>;

/**
 * Turn a conversation's saved rows into thread messages: its system prompt
 * first, when it has one, then the rows with tool rows folded into the
 * assistant rows that called them.
 *
 * Shared by every path that shows what is saved (opening a conversation, a
 * run's end, a delete) so they fold tool rows and rebuild content parts
 * identically; two of them once diverged, and one dropped tool blocks.
 * Each row keeps its database id in its message id (`db-<id>`).
 */
export function buildThreadMessages(
  dbMessages: ChatMessage[],
  conversation: ThreadConversation | null,
  conversationId: number,
): ThreadMessageLike[] {
  const prompt = conversation?.system_prompt?.trim();
  const systemPromptMessage: ThreadMessageLike[] = prompt && conversation
    ? [{
        id: `system-${conversation.id}`,
        role: 'system',
        content: [{ type: 'text' as const, text: prompt }],
        createdAt: new Date(conversation.created_at),
      }]
    : [];

  return [
    ...systemPromptMessage,
    ...foldToolMessages(dbMessages).map<ThreadMessageLike>((message) =>
      buildLoadedMessage(message, conversationId)
    ),
  ];
}
