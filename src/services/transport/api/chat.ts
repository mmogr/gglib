/**
 * Chat API module.
 * Handles conversations and messages for the chat feature.
 */

import { get, post, put, del } from './client';
import { sanitizeMessagesForLlamaServer } from '../sanitizeMessages';
import { parseGeneratedTitle } from '../parseTitleResponse';
import type { ConversationId, MessageId } from '../types/ids';
import type {
  ConversationSummary,
  ChatMessage,
  CreateConversationParams,
  SaveMessageParams,
  DeleteMessageResult,
  GenerateTitleParams,
} from '../types/chat';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../types/chat';

// Re-export the constant for convenience
export { DEFAULT_TITLE_GENERATION_PROMPT };

/**
 * List all conversations.
 */
export async function listConversations(): Promise<ConversationSummary[]> {
  return get<ConversationSummary[]>('/api/conversations');
}

/**
 * Create a new conversation.
 * Returns the new conversation ID.
 */
export async function createConversation(
  params: CreateConversationParams
): Promise<ConversationId> {
  const response = await post<{ id: ConversationId }>(
    '/api/conversations',
    {
      title: params.title,
      model_id: params.modelId,
      system_prompt: params.systemPrompt,
    }
  );
  return response.id;
}

/**
 * Update a conversation's title.
 */
export async function updateConversationTitle(
  id: ConversationId,
  title: string
): Promise<void> {
  await put<void>(`/api/conversations/${id}`, { title });
}

/**
 * Update a conversation's system prompt.
 */
export async function updateConversationSystemPrompt(
  id: ConversationId,
  systemPrompt: string | null
): Promise<void> {
  await put<void>(`/api/conversations/${id}`, { system_prompt: systemPrompt });
}

/**
 * Delete a conversation.
 */
export async function deleteConversation(id: ConversationId): Promise<void> {
  await del<void>(`/api/conversations/${id}`);
}

/**
 * Get all messages for a conversation.
 */
export async function getMessages(conversationId: ConversationId): Promise<ChatMessage[]> {
  return get<ChatMessage[]>(`/api/conversations/${conversationId}/messages`);
}

/**
 * Save a new message.
 * Returns the new message ID.
 */
export async function saveMessage(params: SaveMessageParams): Promise<MessageId> {
  const response = await post<{ id: MessageId }>(
    '/api/messages',
    {
      conversation_id: params.conversationId,
      role: params.role,
      content: params.content,
    }
  );
  return response.id;
}

/**
 * Update a message's content.
 */
export async function updateMessage(id: MessageId, content: string): Promise<void> {
  await put<void>(`/api/messages/${id}`, { content });
}

/**
 * Delete a message and all subsequent messages.
 */
export async function deleteMessage(id: MessageId): Promise<DeleteMessageResult> {
  return del<DeleteMessageResult>(`/api/messages/${id}`);
}

/**
 * Generate a chat title using the served LLM.
 */
export async function generateChatTitle(params: GenerateTitleParams): Promise<string> {
  const { serverPort, messages, prompt = DEFAULT_TITLE_GENERATION_PROMPT } = params;
  
  const sanitizedMessages = sanitizeMessagesForLlamaServer(messages);
  const llamaMessages = [
    ...sanitizedMessages,
    {
      role: 'user' as const,
      content: prompt,
    },
  ];

  const response = await fetch(`http://127.0.0.1:${serverPort}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      messages: llamaMessages,
      temperature: 0.7,
      max_tokens: 20,
    }),
  });

  if (!response.ok) {
    throw new Error(`Title generation failed: ${response.statusText}`);
  }

  const data = await response.json();
  const rawTitle = data.choices?.[0]?.message?.content || 'New Chat';
  return parseGeneratedTitle(rawTitle);
}
