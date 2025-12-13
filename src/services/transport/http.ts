/**
 * HTTP transport implementation.
 * Uses fetch to communicate with the Axum backend via REST API.
 */

import { HF_SEARCH_PATH, HF_QUANTIZATIONS_PATH, HF_TOOL_SUPPORT_PATH } from '../api/routes';
import type { Transport } from './types';
import type { ModelId, HfModelId, McpServerId, DownloadId } from './types/ids';
import type { Unsubscribe, EventHandler } from './types/common';
import type { 
  AddModelParams, 
  UpdateModelParams, 
  SearchModelsParams,
  GgufModel,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  ServeConfig,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantizationsResponse,
  HfToolSupportResponse,
  ModelFilterOptions,
} from './types/models';
import type { ServeResponse, ServerInfo } from './types/servers';
import type { ProxyConfig, ProxyStatus } from './types/proxy';
import type { DownloadQueueStatus, QueueDownloadParams, QueueDownloadResponse } from './types/downloads';
import type { 
  McpServer, 
  NewMcpServer,
  UpdateMcpServer,
  McpServerInfo, 
  McpTool, 
  McpToolResult 
} from './types/mcp';
import type { AppEventType, AppEventMap } from './types/events';
import type { AppSettings, UpdateSettingsRequest } from './types/settings';
import type {
  ConversationSummary,
  ChatMessage,
  CreateConversationParams,
  SaveMessageParams,
  DeleteMessageResult,
  GenerateTitleParams,
} from './types/chat';
import type { ConversationId, MessageId } from './types/ids';
import { DEFAULT_TITLE_GENERATION_PROMPT } from './types/chat';
import { readData, TransportError } from './errors';
import { subscribeSseEvent } from './events/sse';
import { sanitizeMessagesForLlamaServer } from './sanitizeMessages';
import { parseGeneratedTitle } from './parseTitleResponse';

/**
 * HTTP REST transport implementation.
 */
export class HttpTransport implements Transport {
  private readonly baseUrl: string;

  constructor(baseUrl: string = '') {
    this.baseUrl = baseUrl;
  }

  private url(path: string): string {
    return `${this.baseUrl}${path}`;
  }

  private async get<T>(path: string): Promise<T> {
    const response = await fetch(this.url(path));
    return readData<T>(response);
  }

  private async post<T>(path: string, body?: unknown): Promise<T> {
    const response = await fetch(this.url(path), {
      method: 'POST',
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });
    return readData<T>(response);
  }

  private async put<T>(path: string, body: unknown): Promise<T> {
    try {
      const url = this.url(path);
      console.debug('[http.put] request:', { url, path, body });
      const response = await fetch(url, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      console.debug('[http.put] response:', { status: response.status, ok: response.ok, statusText: response.statusText });
      const result = await readData<T>(response);
      console.debug('[http.put] readData succeeded');
      return result;
    } catch (error) {
      console.error('[http.put] error:', error);
      throw error;
    }
  }

  private async delete<T>(path: string): Promise<T> {
    const response = await fetch(this.url(path), { method: 'DELETE' });
    return readData<T>(response);
  }

  // ============================================================================
  // Models
  // ============================================================================

  async listModels(): Promise<GgufModel[]> {
    return this.get<GgufModel[]>('/api/models');
  }

  async getModel(id: ModelId): Promise<GgufModel | null> {
    try {
      return await this.get<GgufModel>(`/api/models/${id}`);
    } catch (error) {
      if (TransportError.hasCode(error, 'NOT_FOUND')) {
        return null;
      }
      throw error;
    }
  }

  async addModel(params: AddModelParams): Promise<GgufModel> {
    return this.post<GgufModel>('/api/models', {
      file_path: params.filePath,
      name: params.name,
    });
  }

  async removeModel(id: ModelId): Promise<void> {
    await this.delete<void>(`/api/models/${id}`);
  }

  async updateModel(params: UpdateModelParams): Promise<GgufModel> {
    return this.put<GgufModel>(`/api/models/${params.id}`, {
      name: params.name,
    });
  }

  async searchModels(params: SearchModelsParams): Promise<GgufModel[]> {
    const queryParams = new URLSearchParams();
    if (params.query) queryParams.set('query', params.query);
    if (params.tags?.length) queryParams.set('tags', params.tags.join(','));
    if (params.quantizations?.length) queryParams.set('quantizations', params.quantizations.join(','));
    if (params.minParams !== undefined) queryParams.set('min_params', String(params.minParams));
    if (params.maxParams !== undefined) queryParams.set('max_params', String(params.maxParams));
    
    const queryString = queryParams.toString();
    const path = queryString ? `/api/models/search?${queryString}` : '/api/models/search';
    return this.get<GgufModel[]>(path);
  }

  async getModelFilterOptions(): Promise<ModelFilterOptions> {
    return this.get<ModelFilterOptions>('/api/models/filter-options');
  }

  async browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
    return this.post<HfSearchResponse>(HF_SEARCH_PATH, params);
  }

  async getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse> {
    return this.get<HfQuantizationsResponse>(`${HF_QUANTIZATIONS_PATH}/${encodeURIComponent(modelId)}`);
  }

  async getHfToolSupport(modelId: HfModelId): Promise<HfToolSupportResponse> {
    return this.get<HfToolSupportResponse>(`${HF_TOOL_SUPPORT_PATH}/${encodeURIComponent(modelId)}`);
  }

  async getSystemMemory(): Promise<SystemMemoryInfo> {
    return this.get<SystemMemoryInfo>('/api/system/memory');
  }

  async getModelsDirectory(): Promise<ModelsDirectoryInfo> {
    return this.get<ModelsDirectoryInfo>('/api/settings/models-directory');
  }

  async setModelsDirectory(path: string): Promise<void> {
    await this.put<void>('/api/settings/models-directory', { path });
  }

  // ============================================================================
  // Tags
  // ============================================================================

  async listTags(): Promise<string[]> {
    return this.get<string[]>('/api/tags');
  }

  async getModelTags(modelId: ModelId): Promise<string[]> {
    return this.get<string[]>(`/api/models/${modelId}/tags`);
  }

  async addModelTag(modelId: ModelId, tag: string): Promise<void> {
    await this.post<void>(`/api/models/${modelId}/tags`, { tag });
  }

  async removeModelTag(modelId: ModelId, tag: string): Promise<void> {
    await this.delete<void>(`/api/models/${modelId}/tags/${encodeURIComponent(tag)}`);
  }

  // ============================================================================
  // Settings
  // ============================================================================

  async getSettings(): Promise<AppSettings> {
    return this.get<AppSettings>('/api/settings');
  }

  async updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
    return this.put<AppSettings>('/api/settings', settings);
  }

  // ============================================================================
  // Servers
  // ============================================================================

  async serveModel(config: ServeConfig): Promise<ServeResponse> {
    return this.post<ServeResponse>('/api/servers/start', config);
  }

  async stopServer(modelId: ModelId): Promise<void> {
    await this.post<void>('/api/servers/stop', { model_id: modelId });
  }

  async listServers(): Promise<ServerInfo[]> {
    return this.get<ServerInfo[]>('/api/servers');
  }

  // ============================================================================
  // Proxy
  // ============================================================================

  async getProxyStatus(): Promise<ProxyStatus> {
    return this.get<ProxyStatus>('/api/proxy/status');
  }

  async startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus> {
    return this.post<ProxyStatus>('/api/proxy/start', config);
  }

  async stopProxy(): Promise<void> {
    await this.post<void>('/api/proxy/stop');
  }

  // ============================================================================
  // Downloads
  // ============================================================================

  async getDownloadQueue(): Promise<DownloadQueueStatus> {
    // Backend returns QueueSnapshot format, we need to transform to DownloadQueueStatus
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const snapshot = await this.get<any>('/api/downloads/queue');
    
    // Backend shape: { items: [...], max_size, active_count, pending_count, recent_failures }
    // Frontend expects: { current, pending, failed, max_size }
    const items = snapshot.items || [];
    const current = items.find((item: { status: string }) => item.status === 'downloading') || null;
    const pending = items.filter((item: { status: string }) => item.status === 'queued');
    const failed = (snapshot.recent_failures || []).concat(
      items.filter((item: { status: string }) => item.status === 'failed' || item.status === 'cancelled')
    );
    
    return {
      current,
      pending,
      failed,
      max_size: snapshot.max_size || 10,
    };
  }

  async queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
    return this.post<QueueDownloadResponse>('/api/downloads/queue', {
      model_id: params.modelId,
      // Use 'quant' as the canonical field name (backend also accepts 'quantization' alias)
      quant: params.quantization,
      target_path: params.targetPath,
    });
  }

  async cancelDownload(id: DownloadId): Promise<void> {
    await this.post<void>(`/api/downloads/${encodeURIComponent(id)}/cancel`);
  }

  async removeFromQueue(id: DownloadId): Promise<void> {
    await this.post<void>(`/api/downloads/${encodeURIComponent(id)}/remove`);
  }

  async clearFailedDownloads(): Promise<void> {
    await this.post<void>('/api/downloads/clear-failed');
  }

  async cancelShardGroup(groupId: string): Promise<void> {
    await this.post<void>(`/api/downloads/groups/${encodeURIComponent(groupId)}/cancel`);
  }

  async reorderQueue(ids: DownloadId[]): Promise<void> {
    await this.post<void>('/api/downloads/reorder-full', { ids });
  }

  // ============================================================================
  // MCP
  // ============================================================================

  async listMcpServers(): Promise<McpServerInfo[]> {
    return this.get<McpServerInfo[]>('/api/mcp/servers');
  }

  async addMcpServer(server: NewMcpServer): Promise<McpServer> {
    return this.post<McpServer>('/api/mcp/servers', server);
  }

  async updateMcpServer(id: McpServerId, updates: UpdateMcpServer): Promise<McpServer> {
    return this.put<McpServer>(`/api/mcp/servers/${id}`, updates);
  }

  async removeMcpServer(id: McpServerId): Promise<void> {
    await this.delete<void>(`/api/mcp/servers/${id}`);
  }

  async startMcpServer(id: McpServerId): Promise<McpTool[]> {
    return this.post<McpTool[]>(`/api/mcp/servers/${id}/start`);
  }

  async stopMcpServer(id: McpServerId): Promise<void> {
    await this.post<void>(`/api/mcp/servers/${id}/stop`);
  }

  async listMcpTools(): Promise<McpTool[]> {
    return this.get<McpTool[]>('/api/mcp/tools');
  }

  async callMcpTool(
    serverId: McpServerId,
    toolName: string,
    args: Record<string, unknown>
  ): Promise<McpToolResult> {
    return this.post<McpToolResult>('/api/mcp/tools/call', {
      server_id: serverId,
      tool_name: toolName,
      arguments: args,
    });
  }

  // ============================================================================
  // Chat
  // ============================================================================

  async listConversations(): Promise<ConversationSummary[]> {
    return this.get<ConversationSummary[]>('/api/conversations');
  }

  async createConversation(params: CreateConversationParams): Promise<ConversationId> {
    return this.post<ConversationId>('/api/conversations', {
      title: params.title || 'New Chat',
      model_id: params.modelId ?? null,
      system_prompt: params.systemPrompt ?? null,
    });
  }

  async updateConversationTitle(id: ConversationId, title: string): Promise<void> {
    try {
      console.debug('[http] updateConversationTitle called:', { id, title, titleLength: title.length });
      await this.put<void>(`/api/conversations/${id}`, { title });
      console.debug('[http] updateConversationTitle succeeded');
    } catch (error) {
      console.error('[http] updateConversationTitle failed:', {
        error,
        errorName: error instanceof Error ? error.name : typeof error,
        errorMessage: error instanceof Error ? error.message : String(error),
        id,
        title,
      });
      throw error;
    }
  }

  async updateConversationSystemPrompt(
    id: ConversationId,
    systemPrompt: string | null
  ): Promise<void> {
    await this.put<void>(`/api/conversations/${id}`, { system_prompt: systemPrompt });
  }

  async deleteConversation(id: ConversationId): Promise<void> {
    await this.delete<void>(`/api/conversations/${id}`);
  }

  async getMessages(conversationId: ConversationId): Promise<ChatMessage[]> {
    return this.get<ChatMessage[]>(`/api/conversations/${conversationId}/messages`);
  }

  async saveMessage(params: SaveMessageParams): Promise<MessageId> {
    return this.post<MessageId>('/api/messages', {
      conversation_id: params.conversationId,
      role: params.role,
      content: params.content,
    });
  }

  async updateMessage(id: MessageId, content: string): Promise<void> {
    await this.put<void>(`/api/messages/${id}`, { content });
  }

  async deleteMessage(id: MessageId): Promise<DeleteMessageResult> {
    const result = await this.delete<{ deleted_count: number }>(`/api/messages/${id}`);
    return { deletedCount: result.deleted_count };
  }

  async generateChatTitle(params: GenerateTitleParams): Promise<string> {
    if (!params.serverPort) {
      throw new TransportError('VALIDATION', 'No server running. Please serve a model first.');
    }

    if (params.messages.length === 0) {
      throw new TransportError('VALIDATION', 'Cannot generate title for empty conversation.');
    }

    const prompt = params.prompt ?? DEFAULT_TITLE_GENERATION_PROMPT;

    // Filter and sanitize messages for llama-server
    // - Exclude system messages (they're handled separately in llama-server)
    // - Sanitize to strip <think> tags and remove unsupported fields
    // - Filter out any messages with empty content
    const validMessages = sanitizeMessagesForLlamaServer(
      params.messages
        .filter((m) => m.role !== 'system')
        .map((m) => ({
          role: m.role,
          content: m.content ?? '',
        }))
    ).filter((m) => m.content.length > 0);

    if (validMessages.length === 0) {
      throw new TransportError('VALIDATION', 'No valid messages to generate title from.');
    }

    const response = await fetch(this.url('/api/chat'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        port: params.serverPort,
        model: 'default',
        messages: [
          ...validMessages,
          { role: 'user', content: prompt },
        ],
        stream: false,
      }),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new TransportError('INTERNAL', `Failed to generate title: ${errorText}`);
    }

    const data = await response.json();
    const rawContent = data.choices?.[0]?.message?.content;

    if (!rawContent || !rawContent.trim()) {
      throw new TransportError('INTERNAL', 'Model returned an empty title.');
    }

    // Debug log to see what the model actually returns
    console.debug('[title-gen] raw model response:', rawContent);

    // Parse with robust error handling
    try {
      const title = parseGeneratedTitle(rawContent);
      console.debug('[title-gen] parsed title:', title);
      return title;
    } catch (error) {
      console.error('[title-gen] parsing failed:', error, 'raw:', rawContent);
      // Fall back to basic cleaning if parsing fails
      return rawContent.trim().slice(0, 100) || 'New Chat';
    }
  }

  // ============================================================================
  // Events
  // ============================================================================

  subscribe<K extends AppEventType>(
    event: K,
    handler: EventHandler<AppEventMap[K]>
  ): Unsubscribe {
    return subscribeSseEvent(event, handler, this.baseUrl);
  }
}
