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
  // Always strip system and tool rows: system prompt is sourced from the
  // conversation record, and tool content is embedded in assistant messages.
  if (toolResultByCallId.size === 0) {
    return messages.filter((m) => m.role !== 'tool' && m.role !== 'system');
  }

  const result: ChatMessage[] = [];

  for (const msg of messages) {
    // Strip tool and system rows — tool content is embedded in assistants,
    // and system prompt is sourced from the conversation record.
    if (msg.role === 'tool' || msg.role === 'system') continue;

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
 * `metadata.contentParts` so they survive the DB round-trip. Reasoning text
 * stored in `metadata.thinking` is injected as a `{type:'reasoning'}` part.
 *
 * Backwards compatibility: if the content column contains legacy `<think>` tags
 * (from before reasoning was stored in metadata), they are parsed and converted
 * to structured reasoning parts.
 */
export function buildLoadedMessage(
  msg: ChatMessage,
  conversationId: number,
): ThreadMessageLike {
  const storedParts = msg.metadata?.contentParts as SerializableContentPart[] | undefined;
  const isDeepResearch = msg.metadata?.isDeepResearch === true;
  const thinkingText = msg.metadata?.thinking as string | undefined;
  const thinkingDuration = msg.metadata?.thinkingDurationSeconds as number | null | undefined;

  const custom: Record<string, unknown> = isDeepResearch
    ? {
        dbId: msg.id,
        conversationId,
        isDeepResearch: true,
        researchState: msg.metadata?.researchState,
      }
    : { dbId: msg.id, conversationId };

  if (thinkingDuration != null) {
    custom.thinkingDurationSeconds = thinkingDuration;
  }

  let content = reconstructContent(msg.content, storedParts ?? null);

  // Inject reasoning from metadata.thinking (new path)
  if (thinkingText) {
    const parts: Array<Record<string, unknown>> =
      typeof content === 'string'
        ? content.trim() ? [{ type: 'text' as const, text: content }] : []
        : [...(content as unknown as Array<Record<string, unknown>>)];
    parts.unshift({ type: 'reasoning', text: thinkingText });
    content = parts as unknown as ThreadMessageLike['content'];
  } else if (msg.role === 'assistant' && typeof content === 'string') {
    // Backwards compat: parse legacy <think> tags embedded in content
    const legacyParsed = parseLegacyThinkingTags(content);
    if (legacyParsed) {
      const parts: Array<Record<string, unknown>> = [];
      parts.push({ type: 'reasoning', text: legacyParsed.thinking });
      if (legacyParsed.durationSeconds != null) {
        custom.thinkingDurationSeconds = legacyParsed.durationSeconds;
      }
      if (legacyParsed.content.trim()) {
        parts.push({ type: 'text', text: legacyParsed.content });
      }
      content = parts as unknown as ThreadMessageLike['content'];
    }
  }

  return {
    id: `db-${msg.id}`,
    role: msg.role as 'user' | 'assistant',
    content,
    createdAt: new Date(msg.created_at),
    metadata: { custom },
  };
}

// ============================================================================
// Legacy <think> tag parser (backwards compatibility for old messages)
// ============================================================================

/**
 * Parse legacy `<think>` tags from content text.
 * Returns null if no tags are found.
 */
function parseLegacyThinkingTags(
  text: string,
): { thinking: string; content: string; durationSeconds: number | null } | null {
  const match = text.match(
    /^<think(?:\s+duration="([\d.]+)")?\s*>([\s\S]*?)<\/think>\s*/,
  );
  if (!match) return null;

  const durationStr = match[1];
  const thinking = match[2]?.trim();
  if (!thinking) return null;

  return {
    thinking,
    content: text.slice(match[0].length),
    durationSeconds: durationStr ? parseFloat(durationStr) : null,
  };
}
