/**
 * Chat API module.
 * Handles conversations and messages for the chat feature.
 */

import { get, post, put, del, getAuthenticatedFetchConfig } from './client';
import { sanitizeMessagesForLlamaServer } from '../sanitizeMessages';
import { parseGeneratedTitle } from '../parseTitleResponse';
import type { ConversationId, MessageId } from '../types/ids';
import type {
  ConversationSummary,
  ChatMessage,
  CreateConversationParams,
  GenerateTitleParams,
} from '../types/chat';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../types/chat';
import type { CreateConversationRequest } from '../../../types/generated/CreateConversationRequest';

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
 * Returns the new conversation ID, which the route sends as a bare number.
 */
export async function createConversation(
  params: CreateConversationParams
): Promise<ConversationId> {
  const body: CreateConversationRequest = {
    title: params.title,
    model_id: params.modelId ?? null,
    system_prompt: params.systemPrompt ?? null,
    model: params.model ?? null,
  };
  return post<ConversationId>('/api/conversations', body);
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
 * Delete a message and all subsequent messages.
 * Returns how many were deleted, which the route sends as a bare number.
 */
export async function deleteMessage(id: MessageId): Promise<number> {
  return del<number>(`/api/messages/${id}`);
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

  const { baseUrl, headers: authHeaders } = await getAuthenticatedFetchConfig();
  const response = await fetch(`${baseUrl}/api/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders },
    body: JSON.stringify({
      port: serverPort,
      messages: llamaMessages,
      temperature: 0.7,
      max_tokens: 20,
      stream: false,
    }),
  });

  if (!response.ok) {
    throw new Error(`Title generation failed: ${response.statusText}`);
  }

  const data = await response.json();
  const rawTitle = data.choices?.[0]?.message?.content || 'New Chat';
  return parseGeneratedTitle(rawTitle);
}
