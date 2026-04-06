/**
 * Wire-format types and conversion for the backend `/api/agent/chat` endpoint.
 *
 * The backend expects a flat `AgentMessage[]` (OpenAI multi-turn format) while
 * the UI stores messages as `GglibMessage[]` with rich content-part arrays.
 * {@link convertToWireMessages} performs that translation.
 *
 * @module wireMessages
 */

import { appLogger } from '../../services/platform';
import { extractParts, type GglibMessage, type GglibToolCallPart, type TextPart } from '../../types/messages';

// ---------------------------------------------------------------------------
// Type guards
// ---------------------------------------------------------------------------

/** Narrow an unknown value to a plain JSON-object args record. */
function isObjectArgs(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

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
        ? extractParts(msg.content)
            .filter((p): p is TextPart => p.type === 'text')
            .map(p => p.text)
            .join('')
        : (msg.content as string) ?? '';
      result.push({ role: msg.role, content });

    } else if (msg.role === 'assistant') {
      const parts = extractParts(msg.content);
      // Content may be a plain string (DB-loaded messages without structured
      // contentParts) or an array of parts (GUI-created messages).  Handle
      // both so we never lose the assistant's text.
      const text = !Array.isArray(msg.content)
        ? (msg.content as string) ?? ''
        : parts
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
      const allToolCallParts = parts.filter(p => p.type === 'tool-call');
      const reasoningCount = parts.filter(p => p.type === 'reasoning').length;
      if (reasoningCount > 0) {
        appLogger.debug('hook.runtime', 'convertToWireMessages: excluded reasoning parts (not sent to backend)', { reasoningCount });
      }
      const completedToolCallParts = allToolCallParts.filter(
        (p): p is GglibToolCallPart & { toolCallId: string; toolName: string; result: unknown } =>
          p.type === 'tool-call' &&
          p.toolCallId != null &&
          p.toolName != null &&
          p.result !== undefined,
      );
      const droppedCount = allToolCallParts.length - completedToolCallParts.length;
      if (droppedCount > 0) {
        appLogger.debug('hook.runtime', 'convertToWireMessages: dropped in-flight tool calls (no result yet)', { droppedCount });
      }

      const toolCalls: AgentWireToolCall[] = completedToolCallParts.map(p => ({
        id: p.toolCallId,
        name: p.toolName,
        // `args` is always populated by addToolCallPart; the guard is a
        // defensive fallback for any part constructed outside that path.
        arguments: isObjectArgs(p.args) ? p.args : {},
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
