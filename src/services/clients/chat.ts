/**
 * Chat client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/chat
 */

import { getTransport } from '../transport';
import type { ConversationId, MessageId, ModelId } from '../transport/types/ids';
import type {
  ConversationSummary,
  ChatMessage,
  ChatMessageMetadata,
  CreateConversationParams,
  SaveMessageParams,
  DeleteMessageResult,
  GenerateTitleParams,
} from '../transport/types/chat';

// Re-export types for consumer convenience
export type {
  ConversationSummary,
  ChatMessage,
  ChatMessageMetadata,
  CreateConversationParams,
  SaveMessageParams,
  DeleteMessageResult,
  GenerateTitleParams,
};

// Re-export the default prompt constant
export { DEFAULT_TITLE_GENERATION_PROMPT } from '../transport/types/chat';

// ============================================================================
// Conversation Operations
// ============================================================================

/**
 * List all conversations.
 */
export async function listConversations(): Promise<ConversationSummary[]> {
  return getTransport().listConversations();
}

/**
 * Create a new conversation.
 * @returns The new conversation ID
 */
export async function createConversation(
  title: string,
  modelId?: ModelId | null,
  systemPrompt?: string | null
): Promise<ConversationId> {
  return getTransport().createConversation({
    title,
    modelId,
    systemPrompt,
  });
}

/**
 * Update a conversation's title.
 */
export async function updateConversationTitle(
  id: ConversationId,
  title: string
): Promise<void> {
  return getTransport().updateConversationTitle(id, title);
}

/**
 * Update a conversation's system prompt.
 */
export async function updateConversationSystemPrompt(
  id: ConversationId,
  systemPrompt: string | null
): Promise<void> {
  return getTransport().updateConversationSystemPrompt(id, systemPrompt);
}

/**
 * Delete a conversation.
 */
export async function deleteConversation(id: ConversationId): Promise<void> {
  return getTransport().deleteConversation(id);
}

// ============================================================================
// Message Operations
// ============================================================================

/**
 * Get all messages for a conversation.
 */
export async function getMessages(
  conversationId: ConversationId
): Promise<ChatMessage[]> {
  return getTransport().getMessages(conversationId);
}

/**
 * Save a new message.
 * @returns The new message ID
 */
export async function saveMessage(
  conversationId: ConversationId,
  role: 'user' | 'assistant' | 'system',
  content: string,
  metadata?: ChatMessageMetadata | null
): Promise<MessageId> {
  return getTransport().saveMessage({
    conversationId,
    role,
    content,
    metadata,
  });
}

/**
 * Update a message's content and/or metadata.
 */
export async function updateMessage(
  id: MessageId,
  content: string,
  metadata?: ChatMessageMetadata | null
): Promise<void> {
  return getTransport().updateMessage(id, { content, metadata });
}

/**
 * Delete a message and all subsequent messages.
 * @returns The number of messages deleted
 */
export async function deleteMessage(id: MessageId): Promise<DeleteMessageResult> {
  return getTransport().deleteMessage(id);
}

// ============================================================================
// AI Title Generation
// ============================================================================

/**
 * Generate a chat title using the served LLM.
 */
export async function generateChatTitle(
  serverPort: number,
  messages: ChatMessage[],
  prompt?: string
): Promise<string> {
  return getTransport().generateChatTitle({
    serverPort,
    messages,
    prompt,
  });
}
