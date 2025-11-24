import { getApiBase } from "../utils/apiBase";

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
}
