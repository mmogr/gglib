/**
 * Message types for gglib chat system.
 * 
 * Uses assistant-ui's ThreadMessageLike directly for proper compatibility.
 * Custom metadata stored in metadata.custom field.
 */

import type { ThreadMessageLike } from '@assistant-ui/react';
import type { ResearchState } from '../hooks/useDeepResearch/types';

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
 * Custom metadata stored in message.metadata.custom
 */
export type GglibMessageCustom = {
  conversationId?: number;
  dbId?: number;
  turnId?: string;
  iteration?: number;
  /** Deep research state - if present, this message is a research artifact */
  researchState?: ResearchState;
  /** Marker that this message represents a deep research session */
  isDeepResearch?: boolean;
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


