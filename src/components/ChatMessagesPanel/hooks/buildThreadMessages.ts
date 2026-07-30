import type { ThreadMessageLike } from '@assistant-ui/react';
import type { ChatMessage, ConversationSummary } from '../../../services/transport';
import { buildLoadedMessage, foldToolMessages } from '../../../hooks/useChatPersistence/buildLoadedMessage';

/**
 * Everything needed to reset the thread runtime from a set of DB rows.
 */
export interface ThreadHydration {
  /** Messages to hand to `threadRuntime.reset()`. */
  messages: ThreadMessageLike[];
  /** Runtime message position -> DB message ID, for edit and delete lookups. */
  dbIdByPosition: Map<number, number>;
  /** Message IDs to seed the persisted-ID set with. */
  seededIds: Set<string>;
}

/**
 * Turn DB rows into thread-runtime messages.
 *
 * Shared by initial hydration and the post-delete reload so both paths fold
 * CLI tool rows and reconstruct content parts identically — the two used to
 * diverge, and the delete path silently dropped tool blocks.
 */
export function buildThreadMessages(
  dbMessages: ChatMessage[],
  conversation: ConversationSummary | null,
  conversationId: number,
): ThreadHydration {
  const folded = foldToolMessages(dbMessages);

  const prompt = conversation?.system_prompt?.trim();
  const systemPromptMessage: ThreadMessageLike[] = prompt && conversation
    ? [{
        id: `system-${conversation.id}`,
        role: 'system',
        content: [{ type: 'text' as const, text: prompt }],
        createdAt: new Date(conversation.created_at),
      }]
    : [];

  const messages: ThreadMessageLike[] = [
    ...systemPromptMessage,
    ...folded.map<ThreadMessageLike>((message) =>
      buildLoadedMessage(message, conversationId)
    ),
  ];

  // Position mapping is built from the unfolded rows: folding only removes
  // tool and system rows, which never carry a delete target of their own.
  const dbIdByPosition = new Map<number, number>();
  const systemOffset = systemPromptMessage.length;
  dbMessages.forEach((msg, idx) => {
    dbIdByPosition.set(systemOffset + idx, msg.id);
  });

  const seededIds = new Set(
    messages
      .map((msg) => msg.id)
      .filter((value): value is string => Boolean(value))
  );

  return { messages, dbIdByPosition, seededIds };
}
