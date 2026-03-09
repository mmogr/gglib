import type { ThreadMessageLike } from '@assistant-ui/react';
import { reconstructContent } from '../../utils/messages';
import type { SerializableContentPart } from '../../utils/messages';
import type { ChatMessage } from '../../services/clients/chat';

/**
 * Convert a raw DB message into a ThreadMessageLike ready for the runtime.
 *
 * Restores structured content parts (tool-call, audio, file, image) stored in
 * `metadata.contentParts` so they survive the DB round-trip. When no parts are
 * stored the plain-text `content` column is used as-is (backward compat).
 */
export function buildLoadedMessage(
  msg: ChatMessage,
  conversationId: number,
): ThreadMessageLike {
  const storedParts = msg.metadata?.contentParts as SerializableContentPart[] | undefined;
  const isDeepResearch = msg.metadata?.isDeepResearch === true;

  const custom = isDeepResearch
    ? {
        dbId: msg.id,
        conversationId,
        isDeepResearch: true,
        researchState: msg.metadata?.researchState,
      }
    : { dbId: msg.id, conversationId };

  return {
    id: `db-${msg.id}`,
    role: msg.role as 'user' | 'assistant',
    content: reconstructContent(msg.content, storedParts ?? null),
    createdAt: new Date(msg.created_at),
    metadata: { custom },
  };
}
