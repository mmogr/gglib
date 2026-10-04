/**
 * Chat transport types.
 * Handles conversations and messages for the chat feature.
 */

import type { Message } from '../../../types/generated/Message';
import type { ModelRef } from '../../../types/generated/ModelRef';
import type { SerializableToolCallPart } from '../../../utils/messages/contentParts';
import type { ConversationId, MessageId, ModelId } from './ids';

// ============================================================================
// DTOs
// ============================================================================

/**
 * Persisted session parameters for a conversation.
 * Mirrors the Rust `ConversationSettings` domain type.
 */
export interface ConversationSettings {
  model_name?: string | null;
  /** The session's model, named by its machine; a resume goes back to it. */
  model?: ModelRef | null;
  temperature?: number | null;
  top_p?: number | null;
  top_k?: number | null;
  max_tokens?: number | null;
  repeat_penalty?: number | null;
  /** The inference profile the session sampled with, by name. */
  profile?: string | null;
  ctx_size?: number | null;
  mlock?: boolean | null;
  tools?: string[] | null;
  tool_timeout_ms?: number | null;
  max_parallel?: number | null;
  max_iterations?: number | null;
  no_tools?: boolean | null;
}

/**
 * Whose chats the chat page shows: this machine's, or those of the machine
 * it is joined to, read through this machine's daemon and never copied.
 */
export type ChatSource = 'this' | 'far';

/**
 * Summary of a conversation for listing.
 */
export interface ConversationSummary {
  id: ConversationId;
  title: string;
  model_id: ModelId | null;
  system_prompt: string | null;
  settings: ConversationSettings | null;
  created_at: string;
  updated_at: string;
}

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
