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

import type { GglibMessage, GglibContent, GglibMessagePart } from '../../types/messages';
import type { AgentToolResult } from '../../types/events/agentEvent';

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
      const parts = Array.isArray(m.content)
        ? ([...m.content] as { type: string; text?: string }[])
        : [];
      // Invariant: the backend emits all `TextDelta` events before any
      // `ToolCallStart` events within a single iteration, so the last part is
      // always a text part (or the array is empty) when this function is called.
      const lastText =
        parts.length > 0 && parts[parts.length - 1].type === 'text'
          ? parts[parts.length - 1]
          : null;

      let nextParts: unknown[];
      if (lastText) {
        nextParts = [
          ...parts.slice(0, -1),
          { type: 'text', text: (lastText.text ?? '') + delta },
        ];
      } else {
        nextParts = [...parts, { type: 'text', text: delta }];
      }
      return { ...m, content: nextParts as GglibContent };
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
      const parts = Array.isArray(m.content)
        ? ([...m.content] as { type: string; text?: string }[])
        : [];
      // Reasoning parts are always appended to the end of the parts array.
      // The backend emits all ReasoningDelta events before TextDelta events
      // within a single iteration, so this is the last part when called.
      const lastReasoning =
        parts.length > 0 && parts[parts.length - 1].type === 'reasoning'
          ? parts[parts.length - 1]
          : null;
      let nextParts: unknown[];
      if (lastReasoning) {
        nextParts = [
          ...parts.slice(0, -1),
          { type: 'reasoning', text: (lastReasoning.text ?? '') + delta },
        ];
      } else {
        nextParts = [...parts, { type: 'reasoning', text: delta }];
      }
      return { ...m, content: nextParts as GglibContent };
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
        ? ([...m.content] as unknown[])
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
  toolResult: AgentToolResult,
): void {
  setMessages(prev =>
    prev.map(m => {
      if (m.id !== messageId) return m;
      const parts = Array.isArray(m.content) ? ([...m.content] as GglibMessagePart[]) : [];
      return {
        ...m,
        content: parts.map(p =>
          p.type === 'tool-call' && p.toolCallId === toolResult.tool_call_id
            ? {
                ...p,
                result: toolResult.success
                  ? toolResult.content
                  : { error: toolResult.content },
                isError: !toolResult.success,
                waitMs: toolResult.wait_ms,
                durationMs: toolResult.duration_ms,
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
      if ((m.metadata as { custom?: { timingFinalized?: boolean } } | undefined)?.custom?.timingFinalized) return m;
      return {
        ...m,
        metadata: {
          ...m.metadata,
          custom: {
            ...(m.metadata as { custom?: Record<string, unknown> } | undefined)?.custom,
            timingFinalized: true,
          },
        },
      };
    }),
  );
}
