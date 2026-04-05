/**
 * Chat transport sub-interface.
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

import type { ResearchState } from '../../../hooks/useDeepResearch/types';
import type { SerializableContentPart } from '../../../utils/messages/contentParts';

/**
 * Metadata attached to a chat message.
 * Used for deep research state, thinking, content parts, custom flags, etc.
 */
export interface ChatMessageMetadata {
  isDeepResearch?: boolean;
  researchState?: ResearchState | null;
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
 * Parameters for updating a conversation.
 */
export interface UpdateConversationParams {
  title?: string;
  systemPrompt?: string | null;
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

/**
 * Chat transport operations.
 */
export interface ChatTransport {
  /** List all conversations. */
  listConversations(): Promise<ConversationSummary[]>;

  /** Create a new conversation. Returns the new conversation ID. */
  createConversation(params: CreateConversationParams): Promise<ConversationId>;

  /** Update a conversation's title. */
  updateConversationTitle(id: ConversationId, title: string): Promise<void>;

  /** Update a conversation's system prompt. */
  updateConversationSystemPrompt(id: ConversationId, systemPrompt: string | null): Promise<void>;

  /** Delete a conversation. */
  deleteConversation(id: ConversationId): Promise<void>;

  /** Get all messages for a conversation. */
  getMessages(conversationId: ConversationId): Promise<ChatMessage[]>;

  /** Save a new message. Returns the new message ID. */
  saveMessage(params: SaveMessageParams): Promise<MessageId>;

  /** Update a message's content and/or metadata. */
  updateMessage(id: MessageId, params: UpdateMessageParams): Promise<void>;

  /** Delete a message and all subsequent messages. */
  deleteMessage(id: MessageId): Promise<DeleteMessageResult>;

  /** Generate a chat title using the served LLM. */
  generateChatTitle(params: GenerateTitleParams): Promise<string>;
}
