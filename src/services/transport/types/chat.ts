/**
 * Chat transport types.
 * Handles conversations and messages for the chat feature.
 */

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
  temperature?: number | null;
  top_p?: number | null;
  top_k?: number | null;
  max_tokens?: number | null;
  repeat_penalty?: number | null;
  ctx_size?: number | null;
  mlock?: boolean | null;
  tools?: string[] | null;
  tool_timeout_ms?: number | null;
  max_parallel?: number | null;
  max_iterations?: number | null;
  no_tools?: boolean | null;
}

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

import type { SerializableContentPart } from '../../../utils/messages/contentParts';

/**
 * Metadata attached to a chat message.
 */
export interface ChatMessageMetadata {
  thinking?: string;
  thinkingDurationSeconds?: number | null;
  contentParts?: SerializableContentPart[];
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
}

/**
 * Parameters for creating a new conversation.
 */
export interface CreateConversationParams {
  title: string;
  modelId?: ModelId | null;
  systemPrompt?: string | null;
}

/**
 * Parameters for saving a message.
 */
export interface SaveMessageParams {
  conversationId: ConversationId;
  role: 'user' | 'assistant' | 'system';
  content: string;
  metadata?: ChatMessageMetadata | null;
}

/**
 * Parameters for updating a message.
 */
export interface UpdateMessageParams {
  content: string;
  metadata?: ChatMessageMetadata | null;
}


/**
 * Result of deleting a message (cascade deletes subsequent messages).
 */
export interface DeleteMessageResult {
  deletedCount: number;
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
