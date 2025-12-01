import { getApiBase } from "../utils/apiBase";

/** Default prompt used for AI-generated chat titles */
export const DEFAULT_TITLE_GENERATION_PROMPT = 
  "Based on this conversation, generate a short descriptive title (max 6 words). " +
  "Respond with ONLY the title text, no quotes, no explanation, no punctuation at the end.";

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ConversationSummary {
  id: number;
  title: string;
  model_id: number | null;
  system_prompt: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChatMessageDto {
  id: number;
  conversation_id: number;
  role: "user" | "assistant" | "system";
  content: string;
  created_at: string;
}

interface CreateConversationPayload {
  title: string;
  model_id: number | null;
  system_prompt: string | null;
}

interface SaveMessagePayload {
  conversation_id: number;
  role: "user" | "assistant" | "system";
  content: string;
}

interface UpdateConversationPayload {
  title?: string;
  system_prompt?: string | null;
}

function buildError(response: Response, fallback: string): Error {
  return new Error(`${fallback} (${response.status} ${response.statusText})`);
}

async function parseResponse<T>(response: Response, fallbackError: string): Promise<T> {
  if (!response.ok) {
    let message = fallbackError;
    try {
      const payload: ApiResponse<T> = await response.json();
      if (payload.error) {
        message = payload.error;
      }
    } catch {
      // ignore parsing errors
    }
    throw new Error(message);
  }

  const data = (await response.json()) as ApiResponse<T>;
  if (!data.success) {
    throw new Error(data.error || fallbackError);
  }
  if (!data.data) {
    throw new Error("Empty response from server");
  }
  return data.data;
}

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const apiBase = await getApiBase();
  return fetch(`${apiBase}${path}`, init);
}

export class ChatService {
  static async listConversations(): Promise<ConversationSummary[]> {
    const response = await apiFetch(`/conversations`);
    return parseResponse<ConversationSummary[]>(response, "Failed to load conversations");
  }

  static async createConversation(
    title: string,
    modelId: number | null = null,
    systemPrompt: string | null = null,
  ): Promise<number> {
    const payload: CreateConversationPayload = {
      title: title || "New Chat",
      model_id: modelId,
      system_prompt: systemPrompt,
    };
    const response = await apiFetch(`/conversations`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    return parseResponse<number>(response, "Failed to create conversation");
  }

  static async updateConversationTitle(conversationId: number, title: string): Promise<void> {
    await ChatService.updateConversation(conversationId, { title });
  }

  static async updateConversationSystemPrompt(
    conversationId: number,
    systemPrompt: string | null,
  ): Promise<void> {
    await ChatService.updateConversation(conversationId, { system_prompt: systemPrompt });
  }

  private static async updateConversation(
    conversationId: number,
    updates: UpdateConversationPayload,
  ): Promise<void> {
    if (!updates || Object.keys(updates).length === 0) {
      return;
    }

    const response = await apiFetch(`/conversations/${conversationId}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(updates),
    });

    if (!response.ok) {
      throw buildError(response, "Failed to update conversation");
    }
  }

  static async deleteConversation(conversationId: number): Promise<void> {
    const response = await apiFetch(`/conversations/${conversationId}`, {
      method: "DELETE",
    });

    if (!response.ok) {
      throw buildError(response, "Failed to delete conversation");
    }
  }

  static async getMessages(conversationId: number): Promise<ChatMessageDto[]> {
    const response = await apiFetch(`/conversations/${conversationId}/messages`);
    return parseResponse<ChatMessageDto[]>(response, "Failed to load messages");
  }

  static async saveMessage(payload: SaveMessagePayload): Promise<number> {
    const response = await apiFetch(`/messages`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    return parseResponse<number>(response, "Failed to save message");
  }

  /**
   * Update a message's content.
   * 
   * @param messageId - The ID of the message to update
   * @param content - The new content for the message
   */
  static async updateMessage(messageId: number, content: string): Promise<void> {
    const response = await apiFetch(`/messages/${messageId}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content }),
    });

    if (!response.ok) {
      throw buildError(response, "Failed to update message");
    }
  }

  /**
   * Delete a message and all subsequent messages in the conversation.
   * This cascade deletion maintains conversation coherence.
   * 
   * @param messageId - The ID of the message to delete
   * @returns The number of messages deleted (including the target and subsequent messages)
   */
  static async deleteMessage(messageId: number): Promise<{ deleted_count: number }> {
    const response = await apiFetch(`/messages/${messageId}`, {
      method: "DELETE",
    });
    return parseResponse<{ deleted_count: number }>(response, "Failed to delete message");
  }

  /**
   * Generate a chat title using the currently served LLM.
   * 
   * Sends the conversation history to the model with a prompt asking for a short,
   * descriptive title. Uses non-streaming mode for simpler handling.
   * 
   * @param serverPort - The port of the currently served model
   * @param messages - The conversation messages to summarize
   * @param prompt - The prompt instructing the LLM how to generate the title
   * @returns The generated title string
   * @throws Error if the request fails or returns an invalid response
   */
  static async generateChatTitle(
    serverPort: number,
    messages: ChatMessageDto[],
    prompt: string = DEFAULT_TITLE_GENERATION_PROMPT,
  ): Promise<string> {
    if (!serverPort) {
      throw new Error("No server running. Please serve a model first.");
    }

    if (messages.length === 0) {
      throw new Error("Cannot generate title for empty conversation.");
    }

    // Build the request with conversation history plus title generation prompt
    const requestBody = {
      port: serverPort,
      model: "default",
      messages: [
        // Include conversation history (filter out system messages for cleaner context)
        ...messages
          .filter((m) => m.role !== "system")
          .map((m) => ({
            role: m.role,
            content: m.content,
          })),
        // Add the title generation instruction
        {
          role: "user",
          content: prompt,
        },
      ],
      stream: false, // Non-streaming for simpler response handling
    };

    const apiBase = await getApiBase();
    const response = await fetch(`${apiBase}/chat`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(requestBody),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Failed to generate title: ${errorText}`);
    }

    const data = await response.json();
    
    // Extract the title from the response
    // OpenAI-compatible format: { choices: [{ message: { content: "..." } }] }
    const generatedTitle = data.choices?.[0]?.message?.content?.trim();

    if (!generatedTitle) {
      throw new Error("Model returned an empty title.");
    }

    // Clean up the title (remove quotes, limit length)
    const cleanedTitle = generatedTitle
      .replace(/^["']|["']$/g, "") // Remove surrounding quotes
      .replace(/\.+$/, "") // Remove trailing periods
      .trim()
      .slice(0, 100); // Limit to reasonable length

    return cleanedTitle || "New Chat";
  }
}
