/**
 * Chat API module.
 * Handles conversations and messages for the chat feature.
 */

import { get, post, put, del } from './client';
import { TransportError } from '../errors';
import { sanitizeMessagesForLlamaServer } from '../sanitizeMessages';
import { parseGeneratedTitle } from '../parseTitleResponse';
import type { ConversationId, MessageId } from '../types/ids';
import type {
  ConversationSummary,
  CreateConversationParams,
  GenerateTitleParams,
  SavedThread,
} from '../types/chat';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../types/chat';
import type { ChatChange } from '../../../types/generated/ChatChange';
import type { ChatChanged } from '../../../types/generated/ChatChanged';
import type { ChatTitleRequest } from '../../../types/generated/ChatTitleRequest';
import type { CreateConversationRequest } from '../../../types/generated/CreateConversationRequest';
import type { UpdateConversationRequest } from '../../../types/generated/UpdateConversationRequest';

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
  const body: UpdateConversationRequest = { title };
  await put<void>(`/api/conversations/${id}`, body);
}

/**
 * Update a conversation's system prompt.
 */
export async function updateConversationSystemPrompt(
  id: ConversationId,
  systemPrompt: string | null
): Promise<void> {
  // No `title` key: the daemon leaves a field it is not sent as it is.
  const body: Partial<UpdateConversationRequest> = { system_prompt: systemPrompt };
  await put<void>(`/api/conversations/${id}`, body);
}

/**
 * Delete a conversation.
 */
export async function deleteConversation(id: ConversationId): Promise<void> {
  await del<void>(`/api/conversations/${id}`);
}

/**
 * A conversation as the page reads it: every saved message, the branch
 * points its family holds along them, and whether it ends in a question
 * nothing answers (ADR 0017).
 */
export async function getThread(conversationId: ConversationId): Promise<SavedThread> {
  return get<SavedThread>(`/api/conversations/${conversationId}/thread`);
}

/**
 * Edit, regenerate or branch a conversation. A change that would rewrite a
 * saved reply is made on a new branch of it, which the answer names; the
 * answer also says whether that chat is now to be answered.
 */
export async function changeConversation(
  conversationId: ConversationId,
  change: ChatChange,
): Promise<ChatChanged> {
  return post<ChatChanged>(`/api/conversations/${conversationId}/changes`, change);
}

/**
 * Delete a message and all subsequent messages.
 * Returns how many were deleted, which the route sends as a bare number.
 */
export async function deleteMessage(id: MessageId): Promise<number> {
  return del<number>(`/api/messages/${id}`);
}

/** The title of a chat whose questions are images alone. */
const IMAGE_CHAT_TITLE = 'Image chat';

/**
 * What a failed title request is shown as. A refusal is said in the title's
 * own sentence, which names the status and not the daemon's reason; anything
 * else is passed on as it came.
 */
function titleFailure(error: unknown): unknown {
  const statusText = TransportError.isTransportError(error)
    ? (error.details as { statusText?: unknown } | undefined)?.statusText
    : undefined;
  return typeof statusText === 'string' ? new Error(`Title generation failed: ${statusText}`) : error;
}

/**
 * Generate a chat title using the served LLM. A chat whose user messages
 * are images with no text gives the model nothing to name it by (it is sent
 * the text alone), so it is titled `IMAGE_CHAT_TITLE` without asking.
 *
 * The daemon answers with the model's text as it gave it, empty when it gave
 * none.
 */
export async function generateChatTitle(params: GenerateTitleParams): Promise<string> {
  const { serverPort, messages, prompt = DEFAULT_TITLE_GENERATION_PROMPT } = params;
  const asked = messages.filter((m) => m.role === 'user');
  if (asked.length > 0 && asked.every((m) => !m.content.trim() && (m.images?.length ?? 0) > 0)) {
    return IMAGE_CHAT_TITLE;
  }

  const body: ChatTitleRequest = {
    port: serverPort,
    messages: [...sanitizeMessagesForLlamaServer(messages), { role: 'user', content: prompt }],
    temperature: 0.7,
    max_tokens: 20,
  };
  const rawTitle = await post<string>('/api/chat', body).catch((error: unknown) => {
    throw titleFailure(error);
  });
  return parseGeneratedTitle(rawTitle || 'New Chat');
}
