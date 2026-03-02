/**
 * Wire-format types and conversion for the backend `/api/agent/chat` endpoint.
 *
 * The backend expects a flat `AgentMessage[]` (OpenAI multi-turn format) while
 * the UI stores messages as `GglibMessage[]` with rich content-part arrays.
 * {@link convertToWireMessages} performs that translation.
 *
 * @module wireMessages
 */

import type { GglibMessage, GglibMessagePart, GglibToolCallPart, TextPart } from '../../types/messages';

// ---------------------------------------------------------------------------
// Wire types (backend request body)
// ---------------------------------------------------------------------------

export type AgentWireMessage =
  | { role: 'system';    content: string }
  | { role: 'user';      content: string }
  | { role: 'assistant'; content: string | null; tool_calls?: AgentWireToolCall[] }
  | { role: 'tool';      tool_call_id: string; content: string };

export interface AgentWireToolCall {
  id: string;
  name: string;
  /** Must be a JSON object — OpenAI tool arguments are always objects. */
  arguments: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

/**
 * Convert `GglibMessage[]` (UI representation) to the flat
 * `AgentMessage[]` wire format expected by the backend.
 *
 * For assistant messages that contain tool-call parts with embedded results,
 * the corresponding `{ role: "tool", … }` entries are emitted immediately
 * after the assistant entry — matching OpenAI's multi-turn format.
 */
export function convertToWireMessages(messages: GglibMessage[]): AgentWireMessage[] {
  const result: AgentWireMessage[] = [];

  for (const msg of messages) {
    if (msg.role === 'system' || msg.role === 'user') {
      const content = Array.isArray(msg.content)
        ? (msg.content as GglibMessagePart[])
            .filter((p): p is TextPart => p.type === 'text')
            .map(p => p.text)
            .join('')
        : (msg.content as string) ?? '';
      result.push({ role: msg.role, content });

    } else if (msg.role === 'assistant') {
      const parts = Array.isArray(msg.content) ? (msg.content as GglibMessagePart[]) : [];
      const text = parts
        .filter((p): p is TextPart => p.type === 'text')
        .map(p => p.text)
        .join('');
      // Only include tool-call parts that have both required string fields AND
      // a result. An assistant message with tool_calls but no corresponding
      // tool-result entries is structurally invalid in the OpenAI wire format.
      // Calls without results (still in-flight or from a partially-captured
      // session) are silently excluded — this is better than sending incoherent
      // context that the model cannot reason over.
      //
      // Note: `reasoning` parts (type === 'reasoning') are intentionally
      // excluded. The backend wire format has no `reasoning` role and the model
      // does not need its own CoT trace as context.
      const completedToolCallParts = parts.filter(
        (p): p is GglibToolCallPart & { toolCallId: string; toolName: string; result: unknown } =>
          p.type === 'tool-call' &&
          p.toolCallId != null &&
          p.toolName != null &&
          p.result !== undefined,
      );

      const toolCalls: AgentWireToolCall[] = completedToolCallParts.map(p => ({
        id: p.toolCallId,
        name: p.toolName,
        // `args` is always populated by addToolCallPart; `?? {}` is a
        // defensive fallback for any part constructed outside that path.
        arguments: (p.args ?? {}) as Record<string, unknown>,
      }));

      result.push({
        role: 'assistant',
        content: text || null,
        ...(toolCalls.length > 0 && { tool_calls: toolCalls }),
      });

      // Emit a `tool` entry for each completed call.
      for (const p of completedToolCallParts) {
        result.push({
          role: 'tool',
          tool_call_id: p.toolCallId,
          content:
            typeof p.result === 'string'
              ? p.result
              : JSON.stringify(p.result),
        });
      }
    }
    // Note: GglibMessage with role === 'tool' does not appear in the
    // ThreadMessageLike format; tool results live inside assistant parts above.
  }

  return result;
}
