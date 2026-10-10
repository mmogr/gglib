/**
 * Message types for gglib chat system.
 * 
 * Uses assistant-ui's ThreadMessageLike directly for proper compatibility.
 * Custom metadata stored in metadata.custom field.
 */

import type { ThreadMessageLike } from '@assistant-ui/react';
import type { TurnMade } from '../utils/messages/turnMade';
import type { AgentToolProgressEvent, AgentWaitingEvent } from './events/agentEvent';
import type { AttachmentInfo } from './generated/AttachmentInfo';

/**
 * Gglib message type - directly uses ThreadMessageLike
 */
export type GglibMessage = ThreadMessageLike;

/**
 * A composer's unsent message: its text and the images attached to it, as
 * the files the page holds. Carried over a model switch and put back.
 */
export interface ChatDraft {
  text: string;
  images: File[];
}

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
 *
 * `artifact` holds the images the tool made, when it made any: put there by
 * `applyToolResult` from a live result and by `foldToolMessages` from a
 * saved tool row, so both show the same tiles.
 *
 * `progress` is how far the tool has got, from its last `tool_progress`
 * event: only on a call drawn from a run and still running. Saved rows do
 * not keep it.
 */
export interface GglibToolCallPart extends ToolCallPart {
  waitMs?: number;
  durationMs?: number;
  artifact?: ToolCallArtifact;
  progress?: ToolProgress;
}

/** A `tool_progress` event, as its call keeps it. */
export type ToolProgress = Omit<AgentToolProgressEvent, 'type' | 'tool_call_id'>;

/** A `waiting` event, as a turn keeps it. */
export type TurnWaiting = Omit<AgentWaitingEvent, 'type'>;

/**
 * The progress a tool-call part carries; none for a part without it.
 * Read with care for the reason `toolCallImages` is.
 */
export function toolCallProgress(part: { progress?: unknown }): ToolProgress | undefined {
  const progress = part.progress as ToolProgress | null | undefined;
  return typeof progress?.stage === 'string' ? progress : undefined;
}

/**
 * What a tool call carries besides its result: the images the tool made,
 * by id and facts, never their bytes. Absent when it made none.
 */
export interface ToolCallArtifact {
  images: AttachmentInfo[];
}

/**
 * The images a tool-call part carries in its `artifact`; none for a part
 * without them. assistant-ui's thread message types `artifact` as unknown,
 * so it is read with care.
 */
export function toolCallImages(part: { artifact?: unknown }): readonly AttachmentInfo[] {
  const images = (part.artifact as { images?: unknown } | null | undefined)?.images;
  return Array.isArray(images) ? (images as AttachmentInfo[]) : [];
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
  /**
   * How far the model has read this turn's prompt, from the run's last
   * `prompt_progress` event. Only on a turn drawn from a run: saved rows do
   * not keep it.
   */
  prompt?: PromptReading;
  /**
   * What the turn is waiting for, from the run's last `waiting` event:
   * only on a turn drawn from a run, and only until its prompt is read.
   */
  waiting?: TurnWaiting;
  /** How the turn was made, from its `turn_usage` event or its saved row. */
  made?: TurnMade;
  /** The paired device that sent a user's turn, from its saved row. */
  device?: string;
};

/** A `prompt_progress` event, as a turn keeps it. */
export interface PromptReading {
  processed: number;
  total: number;
  /** Of `total`, the tokens served from the KV cache. */
  cached: number;
}

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


