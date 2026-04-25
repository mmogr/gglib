/**
 * Message types for gglib chat system.
 * 
 * Uses assistant-ui's ThreadMessageLike directly for proper compatibility.
 * Custom metadata stored in metadata.custom field.
 */

import type { ThreadMessageLike } from '@assistant-ui/react';
import type { SerializableCouncilSession } from './council';

/**
 * Gglib message type - directly uses ThreadMessageLike
 */
export type GglibMessage = ThreadMessageLike;

/**
 * Message content type - can be string or array of parts
 */
export type GglibContent = ThreadMessageLike['content'];

/**
 * Extract message parts from content array
 * ThreadMessageLike['content'] is string | readonly Part[]
 * We extract Part from the array case
 */
export type MessageContent = ThreadMessageLike['content'];
type ContentArray = Extract<MessageContent, readonly any[]>;
export type MessagePart = ContentArray extends readonly (infer P)[] ? P : never;

/**
 * Specific part types
 */
export type ToolCallPart = Extract<MessagePart, { type: 'tool-call' }>;
export type TextPart = Extract<MessagePart, { type: 'text' }>;
export type ReasoningPart = Extract<MessagePart, { type: 'reasoning' }>;

/**
 * Extended tool-call part that adds gglib-specific timing metadata.
 *
 * `waitMs`     — wall-clock time from request dispatch to the first tool byte.
 * `durationMs` — total execution time of the tool call.
 *
 * These fields are stamped onto the part by {@link applyToolResult} once the
 * backend reports completion, allowing the UI to display timing information
 * without a separate side-channel.
 */
export interface GglibToolCallPart extends ToolCallPart {
  waitMs?: number;
  durationMs?: number;
}

/**
 * Union of all content parts that may appear in a GglibMessage.
 *
 * Replaces the raw `MessagePart` where gglib-specific extensions are needed,
 * e.g. to allow `GglibToolCallPart` in an assistant message's content array.
 */
export type GglibMessagePart = Exclude<MessagePart, ToolCallPart> | GglibToolCallPart;

/**
 * Custom metadata stored in message.metadata.custom
 */
export type GglibMessageCustom = {
  conversationId?: number;
  dbId?: number;
  turnId?: string;
  iteration?: number;
  /** Set once the final iteration is complete; triggers persisted transcript regeneration. */
  timingFinalized?: boolean;
  /** Thinking duration in seconds (restored from metadata on load). */
  thinkingDurationSeconds?: number | null;
  /** Whether this message should trigger council mode instead of normal chat. */
  isCouncilMode?: boolean;
  /** Persisted council session data (set on the assistant message that holds the synthesis). */
  councilSession?: SerializableCouncilSession;
};

/**
 * Create a user message
 */
export function mkUserMessage(
  content: GglibContent,
  custom?: GglibMessageCustom
): GglibMessage {
  return {
    id: crypto.randomUUID(),
    role: 'user',
    content,
    createdAt: new Date(),
    ...(custom && { metadata: { custom } }),
  };
}

/**
 * Create an assistant message (initially empty)
 */
export function mkAssistantMessage(
  custom?: GglibMessageCustom
): GglibMessage {
  return {
    id: crypto.randomUUID(),
    role: 'assistant',
    content: [],
    createdAt: new Date(),
    ...(custom && { metadata: { custom } }),
  };
}

/**
 * Extract content parts from a message's content field as a typed array.
 *
 * Consolidates the repeated `Array.isArray(content) ? content as GglibMessagePart[] : []`
 * pattern into a single helper so call sites need no inline type assertions.
 * The internal `as` cast is an unavoidable narrowing from
 * `ThreadMessageLike['content']` (which uses `readonly Part[]`) to
 * `GglibMessagePart[]`; it is sound because `GglibMessagePart` is a
 * supertype of every member of `MessagePart`.
 */
export function extractParts(content: GglibMessage['content']): readonly GglibMessagePart[] {
  return Array.isArray(content) ? (content as readonly GglibMessagePart[]) : [];
}


