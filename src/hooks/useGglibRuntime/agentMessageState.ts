/**
 * React state mutation helpers for an in-flight agent assistant message.
 *
 * Each function takes `setMessages` and a `messageId` and applies a targeted
 * immutable update to the matching `GglibMessage` in state.  They are pure
 * named functions so they can be imported, tested, and re-used independently
 * of the main `streamAgentChat` orchestrator.
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
 * Returns a new array — `parts` is never mutated.
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
    const prev = last as DeltaPart;
    return [
      ...parts.slice(0, -1),
      { ...prev, text: (prev.text ?? '') + delta },
    ];
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
            // argsText is intentionally kept alongside `args` for the DB
            // persistence round-trip in contentParts.ts — it is serialised
            // into metadata.contentParts and restored on load.  Do not remove.
            argsText: JSON.stringify(toolArgs ?? {}),
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
