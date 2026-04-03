import type { ThreadMessageLike } from '@assistant-ui/react';
import { reconstructContent } from '../../utils/messages';
import type { SerializableContentPart, SerializableToolCallPart } from '../../utils/messages';
import type { ChatMessage } from '../../services/clients/chat';

// ============================================================================
// Tool-row folding (CLI agent conversations store tool results as separate rows)
// ============================================================================

/** Metadata shape for assistant messages with tool calls (from CLI persistence). */
interface CliToolCallMeta {
  id: string;
  name: string;
  arguments: unknown;
}

/**
 * Fold `tool`-role DB rows back into the preceding assistant message's
 * `contentParts` so the UI renders them identically to GUI-created
 * conversations.
 *
 * CLI agent persistence stores each `AgentMessage` variant as a separate DB
 * row.  The assistant row carries `metadata.tool_calls` (request info), and
 * each tool row carries `metadata.tool_call_id` (result info).  This function
 * merges the two into `SerializableToolCallPart` entries on the assistant's
 * `contentParts`, then strips the raw tool rows from the list.
 */
export function foldToolMessages(messages: ChatMessage[]): ChatMessage[] {
  // Index tool results by their tool_call_id for O(1) lookup.
  const toolResultByCallId = new Map<string, { content: string }>();
  for (const msg of messages) {
    if (msg.role === 'tool' && msg.metadata?.tool_call_id) {
      toolResultByCallId.set(
        msg.metadata.tool_call_id as string,
        { content: msg.content },
      );
    }
  }

  // Nothing to fold — fast path.
  if (toolResultByCallId.size === 0) return messages;

  const result: ChatMessage[] = [];

  for (const msg of messages) {
    // Strip tool rows — their content is now embedded in the assistant.
    if (msg.role === 'tool') continue;

    // Enrich assistant messages that have tool_calls metadata.
    if (
      msg.role === 'assistant' &&
      Array.isArray(msg.metadata?.tool_calls) &&
      (msg.metadata!.tool_calls as CliToolCallMeta[]).length > 0
    ) {
      const toolCalls = msg.metadata!.tool_calls as CliToolCallMeta[];
      const existingParts =
        (msg.metadata?.contentParts as SerializableContentPart[] | undefined) ?? [];

      const newParts: SerializableContentPart[] = [
        ...existingParts,
        ...toolCalls.map<SerializableToolCallPart>((tc) => {
          const toolResult = toolResultByCallId.get(tc.id);
          return {
            type: 'tool-call',
            toolCallId: tc.id,
            toolName: tc.name,
            args: tc.arguments as Record<string, unknown> | undefined,
            argsText:
              tc.arguments !== undefined
                ? JSON.stringify(tc.arguments)
                : undefined,
            ...(toolResult !== undefined && { result: toolResult.content }),
          };
        }),
      ];

      result.push({
        ...msg,
        metadata: {
          ...msg.metadata,
          contentParts: newParts,
          // Remove tool_calls — now represented as contentParts.
          tool_calls: undefined,
        },
      });
      continue;
    }

    result.push(msg);
  }

  return result;
}

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
