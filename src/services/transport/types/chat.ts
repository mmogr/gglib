/**
 * Chat transport types.
 * Handles conversations and messages for the chat feature.
 */

import type { Conversation } from '../../../types/generated/Conversation';
import type { ConversationSettings } from '../../../types/generated/ConversationSettings';
import type { Message } from '../../../types/generated/Message';
import type { ModelRef } from '../../../types/generated/ModelRef';
import type { SerializableToolCallPart } from '../../../utils/messages/contentParts';
import type { ConversationId, MessageId, ModelId } from './ids';

// ============================================================================
// DTOs
// ============================================================================

/**
 * Persisted session parameters for a conversation: the Rust
 * `ConversationSettings`, as generated from it. A field with no value is
 * left out of the JSON, never `null`.
 */
export type { ConversationSettings };

/**
 * Whose chats the chat page shows: this machine's, or those of the machine
 * it is joined to, read through this machine's daemon and never copied.
 */
export type ChatSource = 'this' | 'far';

/**
 * A conversation as `GET /api/conversations` lists one: the Rust
 * `Conversation`, as generated from it. One with no settings has no
 * `settings` key.
 */
export type ConversationSummary = Conversation;

/**
 * Metadata attached to a chat message.
 */
export interface ChatMessageMetadata {
  thinking?: string;
  thinkingDurationSeconds?: number | null;
  contentParts?: SerializableToolCallPart[];
  [key: string]: unknown;
}

/**
 * A single chat message.
 */
export interface ChatMessage {
  id: MessageId;
  conversation_id: ConversationId;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  created_at: string;
  metadata?: ChatMessageMetadata | null;
  /** The images the message carries, in order, without their bytes; absent when it has none. */
  images?: Message['images'];
}

/**
 * Parameters for creating a new conversation.
 */
export interface CreateConversationParams {
  title: string;
  modelId?: ModelId | null;
  systemPrompt?: string | null;
  /** The model it is for, by its machine: kept as its settings' model, so its machine is fixed. */
  model?: ModelRef | null;
}

/**
 * Parameters for generating a chat title via LLM.
 */
export interface GenerateTitleParams {
  serverPort: number;
  messages: ChatMessage[];
  prompt?: string;
}

/**
 * Default prompt for AI-generated chat titles.
 */
export const DEFAULT_TITLE_GENERATION_PROMPT =
  'Based on this conversation, generate a short descriptive title (max 6 words). ' +
  'Respond with ONLY the title text, no quotes, no explanation, no punctuation at the end.';

// ============================================================================
// Transport Interface
// ============================================================================
