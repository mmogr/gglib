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
  arguments: unknown;
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
      // Filter to tool-call parts that have the required string fields set.
      // addToolCallPart always populates toolCallId and toolName; this guard
      // is a defence against parts constructed via other paths.
      const toolCallParts = parts.filter(
        (p): p is GglibToolCallPart & { toolCallId: string; toolName: string } =>
          p.type === 'tool-call' && p.toolCallId != null && p.toolName != null,
      );
      // Note: `reasoning` parts (type === 'reasoning') are intentionally
      // excluded here.  The backend wire format has no `reasoning` message role
      // and the model does not need its own CoT trace as context.

      const toolCalls: AgentWireToolCall[] = toolCallParts.map(p => ({
        id: p.toolCallId,
        name: p.toolName,
        // `args` is always populated by addToolCallPart; `?? {}` is a
        // defensive fallback for any part constructed outside that path.
        arguments: p.args ?? {},
      }));

      result.push({
        role: 'assistant',
        content: text || null,
        ...(toolCalls.length > 0 && { tool_calls: toolCalls }),
      });

      // Emit a `tool` entry for each completed tool call (result in part).
      for (const p of toolCallParts) {
        if (p.result !== undefined) {
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
    }
    // Note: GglibMessage with role === 'tool' does not appear in the
    // ThreadMessageLike format; tool results live inside assistant parts above.
  }

  return result;
}
