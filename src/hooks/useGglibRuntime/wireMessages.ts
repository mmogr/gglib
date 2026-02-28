/**
 * Wire-format types and conversion for the backend `/api/agent/chat` endpoint.
 *
 * The backend expects a flat `AgentMessage[]` (OpenAI multi-turn format) while
 * the UI stores messages as `GglibMessage[]` with rich content-part arrays.
 * {@link convertToWireMessages} performs that translation.
 *
 * @module wireMessages
 */

import type { GglibMessage } from '../../types/messages';

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
        ? (msg.content as { type: string; text?: string }[])
            .filter(p => p.type === 'text')
            .map(p => p.text ?? '')
            .join('')
        : (msg.content as string) ?? '';
      result.push({ role: msg.role, content });

    } else if (msg.role === 'assistant') {
      const parts = Array.isArray(msg.content)
        ? (msg.content as { type: string; toolCallId?: string; toolName?: string; args?: unknown; argsText?: string; result?: unknown }[])
        : [];
      const text = parts
        .filter(p => p.type === 'text')
        .map(p => (p as { type: string; text?: string }).text ?? '')
        .join('');
      const toolCallParts = parts.filter(p => p.type === 'tool-call');

      const toolCalls: AgentWireToolCall[] = toolCallParts.map(p => ({
        id: p.toolCallId as string,
        name: p.toolName as string,
        arguments: p.args ?? (p.argsText ? JSON.parse(p.argsText as string) : {}),
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
            tool_call_id: p.toolCallId as string,
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
