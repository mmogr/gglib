import type { ModelId, DownloadId, HfModelId, ConversationId } from '../../services/transport/types/ids';
import type { ServeConfig } from '../../types';
import type { CreateConversationParams, ChatMessage } from '../../services/transport/types/chat';
import type { HfSearchRequest, HfSearchResponse } from '../../types';
import type { QueueDownloadParams, DownloadQueueStatus } from '../../services/transport/types/downloads';
import type { ProxyConfig, ProxyStatus } from '../../services/transport/types/proxy';
import type { AppSettings, UpdateSettingsRequest } from '../../types';

import { listModels, addModel, removeModel } from '../../services/clients/models';
import { listServers, serveModel, stopServer, getProxyStatus, startProxy, stopProxy } from '../../services/clients/servers';
import { getDownloadQueue, cancelDownload, removeFromQueue, clearFailedDownloads, cancelShardGroup, reorderQueue, queueDownload } from '../../services/clients/downloads';
import { browseHfModels, getHfQuantizations, getHfToolSupport } from '../../services/clients/huggingface';
import { listConversations, createConversation, getMessages, saveMessage } from '../../services/clients/chat';
import { getSettings, updateSettings } from '../../services/clients/settings';

export const api = {
  // Models
  getModels: listModels,
  addModelPath: async (filePath: string) => addModel({ filePath }),
  removeModel: async (modelId: ModelId) => removeModel(modelId),

  // Servers
  getServers: listServers,
  startServer: async (modelId: ModelId, contextLength?: number) => {
    const config: ServeConfig = {
      id: modelId,
      context_length: contextLength,
    };
    return serveModel(config);
  },
  stopServer: async (modelId: ModelId) => stopServer(modelId),

  // Proxy
  getProxyStatus: async (): Promise<ProxyStatus> => getProxyStatus(),
  startProxy: async (config?: Partial<ProxyConfig>): Promise<ProxyStatus> => startProxy(config),
  stopProxy: async (): Promise<void> => stopProxy(),

  // Downloads
  getDownloadQueue: async (): Promise<DownloadQueueStatus> => getDownloadQueue(),
  queueDownload: async (params: QueueDownloadParams) => queueDownload(params),
  cancelDownload: async (id: DownloadId) => cancelDownload(id),
  removeFromQueue: async (id: DownloadId) => removeFromQueue(id),
  clearFailedDownloads: async () => clearFailedDownloads(),
  cancelShardGroup: async (groupId: string) => cancelShardGroup(groupId),
  reorderQueue: async (ids: DownloadId[]) => reorderQueue(ids),

  // HuggingFace
  browseHfModels: async (params: HfSearchRequest): Promise<HfSearchResponse> => browseHfModels(params),
  getHfQuantizations: async (modelId: HfModelId) => getHfQuantizations(modelId),
  getHfToolSupport: async (modelId: HfModelId) => getHfToolSupport(modelId),

  // Chat persistence + basic inference
  listConversations,
  createConversation: async (params: CreateConversationParams): Promise<ConversationId> =>
    createConversation(params.title, params.modelId, params.systemPrompt),
  getMessages,
  saveMessage: async (conversationId: ConversationId, role: ChatMessage['role'], content: string) =>
    saveMessage(conversationId, role, content),

  // Settings
  getSettings: async (): Promise<AppSettings> => getSettings(),
  updateSettings: async (req: UpdateSettingsRequest): Promise<AppSettings> => updateSettings(req),
};

