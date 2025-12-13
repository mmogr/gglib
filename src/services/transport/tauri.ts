/**
 * Tauri transport implementation.
 * Uses @tauri-apps/api to communicate with the Rust backend via IPC.
 */

import { invoke } from '@tauri-apps/api/core';
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
import { wrapInvoke, TransportError } from './errors';
import { subscribeTauriEvent } from './events/tauri';
import { sanitizeMessagesForLlamaServer } from './sanitizeMessages';
import { parseGeneratedTitle } from './parseTitleResponse';
import { 
  toStartServerRequest, 
  toCreateConversationRequest, 
  toSaveMessageRequest,
  toUpdateConversationRequest,
} from './mappers';

/**
 * Tauri IPC transport implementation.
 * 
 * IMPORTANT: Tauri v2 expects command argument keys in camelCase from JavaScript.
 * The framework automatically converts camelCase to snake_case when mapping to Rust parameters.
 * Example: { conversationId: 123 } in JS → conversation_id: i64 in Rust
 * 
 * Do NOT use snake_case keys in invoke() calls unless the Rust command explicitly
 * uses #[tauri::command(rename_all = "snake_case")].
 * 
 * @see https://v2.tauri.app/develop/calling-rust/
 */
export class TauriTransport implements Transport {
  // ============================================================================
  // Models
  // ============================================================================

  async listModels(): Promise<GgufModel[]> {
    return wrapInvoke(invoke<GgufModel[]>('list_models'));
  }

  async getModel(id: ModelId): Promise<GgufModel | null> {
    const models = await this.listModels();
    return models.find(m => m.id === id) ?? null;
  }

  async addModel(params: AddModelParams): Promise<GgufModel> {
    return wrapInvoke(invoke<GgufModel>('add_model', { 
      filePath: params.filePath,
      name: params.name,
    }));
  }

  async removeModel(id: ModelId): Promise<void> {
    return wrapInvoke(invoke<void>('remove_model', { id }));
  }

  async updateModel(params: UpdateModelParams): Promise<GgufModel> {
    return wrapInvoke(invoke<GgufModel>('update_model', { 
      id: params.id,
      name: params.name,
    }));
  }

  async searchModels(params: SearchModelsParams): Promise<GgufModel[]> {
    return wrapInvoke(invoke<GgufModel[]>('search_models', { 
      query: params.query,
      tags: params.tags,
      quantizations: params.quantizations,
      minParams: params.minParams,
      maxParams: params.maxParams,
    }));
  }

  async getModelFilterOptions(): Promise<ModelFilterOptions> {
    return wrapInvoke(invoke<ModelFilterOptions>('get_model_filter_options'));
  }

  async browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
    return wrapInvoke(invoke<HfSearchResponse>('search_hf_models', { request: params }));
  }

  async getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse> {
    return wrapInvoke(invoke<HfQuantizationsResponse>('get_hf_quantizations', { modelId }));
  }

  async getHfToolSupport(modelId: HfModelId): Promise<HfToolSupportResponse> {
    return wrapInvoke(invoke<HfToolSupportResponse>('get_hf_tool_support', { modelId }));
  }

  async getSystemMemory(): Promise<SystemMemoryInfo> {
    return wrapInvoke(invoke<SystemMemoryInfo>('get_system_memory'));
  }

  async getModelsDirectory(): Promise<ModelsDirectoryInfo> {
    return wrapInvoke(invoke<ModelsDirectoryInfo>('get_models_directory'));
  }

  async setModelsDirectory(path: string): Promise<void> {
    return wrapInvoke(invoke<void>('set_models_directory', { path }));
  }

  // ============================================================================
  // Tags
  // ============================================================================

  async listTags(): Promise<string[]> {
    return wrapInvoke(invoke<string[]>('list_tags'));
  }

  async getModelTags(modelId: ModelId): Promise<string[]> {
    return wrapInvoke(invoke<string[]>('get_model_tags', { modelId }));
  }

  async addModelTag(modelId: ModelId, tag: string): Promise<void> {
    return wrapInvoke(invoke<void>('add_model_tag', { modelId, tag }));
  }

  async removeModelTag(modelId: ModelId, tag: string): Promise<void> {
    return wrapInvoke(invoke<void>('remove_model_tag', { modelId, tag }));
  }

  // ============================================================================
  // Settings
  // ============================================================================

  async getSettings(): Promise<AppSettings> {
    return wrapInvoke(invoke<AppSettings>('get_settings'));
  }

  async updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
    // Type-safe params for Tauri command - ensures we can't accidentally use wrong key
    type UpdateSettingsParams = {
      updates: UpdateSettingsRequest;
    };
    
    const params: UpdateSettingsParams = { updates: settings };
    return wrapInvoke(invoke<AppSettings>('update_settings', params));
  }

  // ============================================================================
  // Servers
  // ============================================================================

  async serveModel(config: ServeConfig): Promise<ServeResponse> {
    return wrapInvoke(invoke<ServeResponse>('serve_model', { 
      id: config.id, 
      request: toStartServerRequest(config) 
    }));
  }

  async stopServer(modelId: ModelId): Promise<void> {
    return wrapInvoke(invoke<void>('stop_server', { modelId }));
  }

  async listServers(): Promise<ServerInfo[]> {
    return wrapInvoke(invoke<ServerInfo[]>('list_servers'));
  }

  // ============================================================================
  // Proxy
  // ============================================================================

  async getProxyStatus(): Promise<ProxyStatus> {
    return wrapInvoke(invoke<ProxyStatus>('get_proxy_status'));
  }

  async startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus> {
    return wrapInvoke(invoke<ProxyStatus>('start_proxy', { config }));
  }

  async stopProxy(): Promise<void> {
    return wrapInvoke(invoke<void>('stop_proxy'));
  }

  // ============================================================================
  // Downloads
  // ============================================================================

  async getDownloadQueue(): Promise<DownloadQueueStatus> {
    return wrapInvoke(invoke<DownloadQueueStatus>('get_download_queue'));
  }

  async queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
    return wrapInvoke(invoke<QueueDownloadResponse>('queue_download', {
      modelId: params.modelId,
      quantization: params.quantization,
      targetPath: params.targetPath,
    }));
  }

  async cancelDownload(id: DownloadId): Promise<void> {
    return wrapInvoke(invoke<void>('cancel_download', { modelId: id }));
  }

  async removeFromQueue(id: DownloadId): Promise<void> {
    return wrapInvoke(invoke<void>('remove_from_download_queue', { modelId: id }));
  }

  async clearFailedDownloads(): Promise<void> {
    return wrapInvoke(invoke<void>('clear_failed_downloads'));
  }

  async cancelShardGroup(groupId: string): Promise<void> {
    return wrapInvoke(invoke<void>('cancel_shard_group', { groupId }));
  }

  async reorderQueue(ids: DownloadId[]): Promise<void> {
    return wrapInvoke(invoke<void>('reorder_download_queue', { ids }));
  }

  // ============================================================================
  // MCP
  // ============================================================================

  async listMcpServers(): Promise<McpServerInfo[]> {
    return wrapInvoke(invoke<McpServerInfo[]>('list_mcp_servers'));
  }

  async addMcpServer(server: NewMcpServer): Promise<McpServer> {
    return wrapInvoke(invoke<McpServer>('add_mcp_server', { server }));
  }

  async updateMcpServer(id: McpServerId, updates: UpdateMcpServer): Promise<McpServer> {
    return wrapInvoke(invoke<McpServer>('update_mcp_server', { id, updates }));
  }

  async removeMcpServer(id: McpServerId): Promise<void> {
    return wrapInvoke(invoke<void>('remove_mcp_server', { id }));
  }

  async startMcpServer(id: McpServerId): Promise<McpTool[]> {
    return wrapInvoke(invoke<McpTool[]>('start_mcp_server', { id }));
  }

  async stopMcpServer(id: McpServerId): Promise<void> {
    return wrapInvoke(invoke<void>('stop_mcp_server', { id }));
  }

  async listMcpTools(): Promise<McpTool[]> {
    return wrapInvoke(invoke<McpTool[]>('list_mcp_tools'));
  }

  async callMcpTool(
    serverId: McpServerId, 
    toolName: string, 
    args: Record<string, unknown>
  ): Promise<McpToolResult> {
    return wrapInvoke(invoke<McpToolResult>('call_mcp_tool', { 
      serverId, 
      toolName, 
      arguments: args 
    }));
  }

  // ============================================================================
  // Chat
  // ============================================================================

  async listConversations(): Promise<ConversationSummary[]> {
    return wrapInvoke(invoke<ConversationSummary[]>('list_conversations'));
  }

  async createConversation(params: CreateConversationParams): Promise<ConversationId> {
    return wrapInvoke(invoke<ConversationId>('create_conversation', {
      request: toCreateConversationRequest(params),
    }));
  }

  async updateConversationTitle(id: ConversationId, title: string): Promise<void> {
    try {
      const payload = toUpdateConversationRequest(title, undefined);
      console.debug('[tauri] updateConversationTitle called:', { id, title, titleLength: title.length, payload });
      const result = await wrapInvoke(invoke<void>('update_conversation', {
        id,
        request: payload,
      }));
      console.debug('[tauri] updateConversationTitle succeeded');
      return result;
    } catch (error) {
      console.error('[tauri] updateConversationTitle failed:', {
        error,
        errorName: error instanceof Error ? error.name : typeof error,
        errorMessage: error instanceof Error ? error.message : String(error),
        errorStack: error instanceof Error ? error.stack : undefined,
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
    return wrapInvoke(invoke<void>('update_conversation', {
      id,
      request: toUpdateConversationRequest(undefined, systemPrompt),
    }));
  }

  async deleteConversation(id: ConversationId): Promise<void> {
    return wrapInvoke(invoke<void>('delete_conversation', { id }));
  }

  async getMessages(conversationId: ConversationId): Promise<ChatMessage[]> {
    return wrapInvoke(invoke<ChatMessage[]>('get_messages', { 
      conversationId,
    }));
  }

  async saveMessage(params: SaveMessageParams): Promise<MessageId> {
    return wrapInvoke(invoke<MessageId>('save_message', {
      request: toSaveMessageRequest(params),
    }));
  }

  async updateMessage(id: MessageId, content: string): Promise<void> {
    return wrapInvoke(invoke<void>('update_message', { id, content }));
  }

  async deleteMessage(id: MessageId): Promise<DeleteMessageResult> {
    const deletedCount = await wrapInvoke(invoke<number>('delete_message', { id }));
    return { deletedCount };
  }

  async generateChatTitle(params: GenerateTitleParams): Promise<string> {
    // For title generation, we proxy through the embedded API server
    // since it requires the llama-server endpoint
    if (!params.serverPort) {
      throw new TransportError(
        'VALIDATION',
        'No server running. Please serve a model first.'
      );
    }

    if (params.messages.length === 0) {
      throw new TransportError(
        'VALIDATION',
        'Cannot generate title for empty conversation.'
      );
    }

    // Use the embedded API port for chat completion
    const apiPort = await wrapInvoke(invoke<number>('get_gui_api_port'));
    
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
      throw new TransportError(
        'VALIDATION',
        'No valid messages to generate title from.'
      );
    }
    
    const response = await fetch(`http://localhost:${apiPort}/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        port: params.serverPort,
        model: 'default',
        messages: [
          ...validMessages,
          {
            role: 'user',
            content:
              params.prompt ??
              'Based on this conversation, generate a short descriptive title (max 6 words). ' +
              'Respond with ONLY the title text, no quotes, no explanation, no punctuation at the end.',
          },
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
    return subscribeTauriEvent(event, handler);
  }
}
