/**
 * React state mutation helpers for an in-flight agent assistant message.
 *
 * Each function takes `setMessages` and a `messageId` and applies a targeted
 * update to the matching `GglibMessage` in state.  They are pure named
 * functions so they can be imported, tested, and re-used independently of the
 * main `streamAgentChat` orchestrator.
 *
 * ## Performance: in-place part mutation
 *
 * Delta functions (`applyTextDelta`, `applyReasoningDelta`) are called once
 * per SSE token — potentially hundreds of times per second during fast
 * streaming.  To avoid GC pressure and UI stuttering, `applyDeltaToLastPart`
 * mutates the last element of the `parts` array **in place** rather than
 * creating a new array on every token.  A shallow clone of the top-level
 * `messages` array (via `prev.map`) is sufficient for React to detect the
 * state change; the inner `parts` array identity is deliberately preserved.
 *
 * @module agentMessageState
 */

import React from 'react';

import type {
  GglibMessage,
  GglibContent,
  GglibMessagePart,
  GglibMessageCustom,
  TextPart,
  ReasoningPart,
} from '../../types/messages';
import type { AgentToolCallCompleteEvent } from '../../types/events/agentEvent';

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/** Parts that carry a `text` payload and can be extended by delta events. */
type DeltaPart = TextPart | ReasoningPart;

/**
 * Append `delta` to the last part whose `type` matches `partType`, or push a
 * new part when no such part exists at the tail of the array.
 *
 * **Mutation strategy:** When appending to an existing trailing part the
 * function mutates `text` in place and returns the *same* array reference.
 * The caller is responsible for creating a new top-level messages array
 * (via `prev.map`) so that React detects the state change.  Only when a
 * brand-new part is needed does the function allocate a new array via
 * spread.
 *
 * This is the common kernel shared by {@link applyTextDelta} and
 * {@link applyReasoningDelta}; the only difference between those two callers
 * is the `partType` string they pass in.
 */
function applyDeltaToLastPart(
  parts: GglibMessagePart[],
  partType: DeltaPart['type'],
  delta: string,
): GglibMessagePart[] {
  const last = parts.at(-1);
  if (last?.type === partType) {
    // Mutate in place — the caller already clones the messages array for React.
    // Cast through a mutable intermediate to bypass the readonly constraint on
    // the imported assistant-ui part types (the readonly is on the array, not
    // semantically on the field; mutation here is intentional — see module doc).
    (last as { type: string; text: string }).text =
      ((last as DeltaPart).text ?? '') + delta;
    return parts;
  }
  return [...parts, { type: partType, text: delta } as GglibMessagePart];
}

// ---------------------------------------------------------------------------
// Text content
// ---------------------------------------------------------------------------

/** Append a text delta to the current message's text part (or create one). */
export function applyTextDelta(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
  delta: string,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      // Invariant: the backend emits all `TextDelta` events before any
      // `ToolCallStart` events within a single iteration, so the last part is
      // always a text part (or the array is empty) when this function is called.
      const parts = Array.isArray(m.content) ? ([...m.content] as GglibMessagePart[]) : [];
      return { ...m, content: applyDeltaToLastPart(parts, 'text', delta) as GglibContent };
    }),
  );
}

/** Append a reasoning/thinking delta to the current message's reasoning part (or create one). */
export function applyReasoningDelta(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
  delta: string,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      // The backend emits all ReasoningDelta events before TextDelta events
      // within a single iteration, so the last part matches 'reasoning' here.
      const parts = Array.isArray(m.content) ? ([...m.content] as GglibMessagePart[]) : [];
      return { ...m, content: applyDeltaToLastPart(parts, 'reasoning', delta) as GglibContent };
    }),
  );
}

/**
 * Replace the text content of a message wholesale (not a delta append).
 *
 * Used by the `final_answer` handler to guarantee the complete answer text
 * is present in the message even if individual `text_delta` events were lost
 * in transit.  Finds the *last* text part and replaces its content, or
 * pushes a new text part if none exists.
 */
export function setFullText(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
  text: string,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      const parts = Array.isArray(m.content) ? ([...m.content] as GglibMessagePart[]) : [];
      // Find the last text part and replace its content.
      const lastTextIdx = parts.findLastIndex(p => p.type === 'text');
      if (lastTextIdx >= 0) {
        const updated = [...parts];
        updated[lastTextIdx] = { ...updated[lastTextIdx], type: 'text', text } as GglibMessagePart;
        return { ...m, content: updated as GglibContent };
      }
      // No text part exists yet — create one.
      return { ...m, content: [...parts, { type: 'text', text } as GglibMessagePart] as GglibContent };
    }),
  );
}

// ---------------------------------------------------------------------------
// Tool calls
// ---------------------------------------------------------------------------

/** Add a pending (no result yet) tool-call part. */
export function addToolCallPart(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
  toolCallId: string,
  toolName: string,
  toolArgs: unknown,
  displayName?: string,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      const parts = Array.isArray(m.content)
        ? ([...m.content] as GglibMessagePart[])
        : [];
      return {
        ...m,
        content: [
          ...parts,
          {
            type: 'tool-call',
            toolCallId,
            toolName,
            args: typeof toolArgs === 'object' ? toolArgs : {},
            ...(displayName ? { displayName } : {}),
          },
        ] as GglibContent,
      };
    }),
  );
}

/** Stamp a tool result onto the matching tool-call part. */
export function applyToolResult(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
  event: AgentToolCallCompleteEvent,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      const parts = Array.isArray(m.content) ? ([...m.content] as GglibMessagePart[]) : [];
      return {
        ...m,
        content: parts.map(p =>
          p.type === 'tool-call' && p.toolCallId === event.result.tool_call_id
            ? {
                ...p,
                result: event.result.success
                  ? event.result.content
                  : { error: event.result.content },
                isError: !event.result.success,
                waitMs: event.wait_ms,
                durationMs: event.execute_duration_ms,
              }
            : p,
        ) as GglibContent,
      };
    }),
  );
}

// ---------------------------------------------------------------------------
// Timing finalization
// ---------------------------------------------------------------------------

/** Mark a message as timing-finalized (triggers persisted transcript regeneration). */
export function finalizeMessageTiming(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  messageId: string,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      const meta = m.metadata as { custom?: GglibMessageCustom } | undefined;
      if (meta?.custom?.timingFinalized) return m;
      return {
        ...m,
        metadata: {
          ...m.metadata,
          custom: {
            ...meta?.custom,
            timingFinalized: true,
          },
        },
      };
    }),
  );
}
