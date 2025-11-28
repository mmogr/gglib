// Model and Server Management Service
// Auto-detects platform: Tauri desktop app uses invoke(), Web UI uses REST API
// Single codebase, no duplication, works in both modes

import { invoke } from "@tauri-apps/api/core";
import {
  GgufModel,
  DownloadConfig,
  ServeConfig,
  DownloadQueueStatus,
} from "../types";
import { getApiBase } from "../utils/apiBase";

// Platform detection
// Check if we're running in Tauri (desktop app) or Web UI
export const isTauriApp = typeof (window as any).__TAURI_INTERNALS__ !== 'undefined' ||
                   typeof (window as any).__TAURI__ !== 'undefined';

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const apiBase = await getApiBase();
  return fetch(`${apiBase}${path}`, init);
}

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

/**
 * Response from queue_download containing position and shard count.
 */
interface QueueDownloadResponse {
  position: number;
  shard_count: number;
}

export class TauriService {
  // Model operations
  static async listModels(): Promise<GgufModel[]> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<GgufModel[]>('list_models');
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models`);
      if (!response.ok) {
        throw new Error(`Failed to fetch models: ${response.statusText}`);
      }
      const data: ApiResponse<GgufModel[]> = await response.json();
      return data.data || [];
    }
  }

  static async getModel(id: number): Promise<GgufModel> {
    if (isTauriApp) {
      // Desktop GUI: Get via listModels
      const models = await this.listModels();
      const model = models.find(m => m.id === id);
      if (!model) throw new Error(`Model ${id} not found`);
      return model;
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/${id}`);
      if (!response.ok) {
        throw new Error(`Failed to fetch model: ${response.statusText}`);
      }
      const data: ApiResponse<GgufModel> = await response.json();
      if (!data.data) {
        throw new Error(`Model ${id} not found`);
      }
      return data.data;
    }
  }

  static async addModel(filePath: string): Promise<string> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<string>('add_model', { filePath });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ file_path: filePath }),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to add model');
      }
      
      const data: ApiResponse<GgufModel> = await response.json();
      return `Model added: ${data.data?.name}`;
    }
  }

  static async removeModel(identifier: string, force: boolean = false): Promise<string> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<string>('remove_model', { identifier, force });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/${identifier}`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ force }),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to remove model');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Model removed successfully';
    }
  }

  static async updateModel(id: number, updates: {
    name?: string;
    quantization?: string;
    file_path?: string;
  }): Promise<GgufModel> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<GgufModel>('update_model', { id, updates });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(updates),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to update model');
      }
      
      const data: ApiResponse<GgufModel> = await response.json();
      if (!data.data) {
        throw new Error('Invalid response from server');
      }
      return data.data;
    }
  }

  // Server operations
  static async serveModel(config: ServeConfig): Promise<{ port: number; message: string }> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      const message = await invoke<string>('serve_model', {
        id: config.id,
        ctxSize: config.ctx_size,
        contextLength: config.context_length,
        mlock: config.mlock || false,
        port: config.port,
        jinja: config.jinja,
      });
      return { port: config.port || 9000, message };
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/${config.id}/start`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          context_length: config.context_length || (config.ctx_size ? parseInt(config.ctx_size) : undefined),
          mlock: config.mlock || false,
          jinja: config.jinja,
        }),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to start server');
      }
      
      const data: ApiResponse<{ port: number; message: string }> = await response.json();
      if (!data.data) {
        throw new Error('Invalid server response');
      }
      return data.data;
    }
  }

  static async stopServer(modelId: number): Promise<string> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<string>('stop_server', { modelId });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/${modelId}/stop`, {
        method: 'POST',
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to stop server');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Server stopped';
    }
  }

  static async listServers(): Promise<any[]> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<any[]>('list_servers');
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/servers`);
      if (!response.ok) {
        throw new Error(`Failed to fetch servers: ${response.statusText}`);
      }
      const data: ApiResponse<any[]> = await response.json();
      return data.data || [];
    }
  }

  // Download and search
  static async downloadModel(config: DownloadConfig): Promise<string> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke (events handled via listen() in component)
      return await invoke<string>('download_model', {
        modelId: config.repo_id,
        quantization: config.quantization,
      });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/download`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model_id: config.repo_id,
          quantization: config.quantization,
        }),
      });
      
      if (!response.ok) {
        let errorMessage = 'Failed to download model';
        try {
          const error: ApiResponse<any> = await response.json();
          errorMessage = error.error || errorMessage;
        } catch {
          // If JSON parsing fails, use status text
          errorMessage = response.statusText || errorMessage;
        }
        throw new Error(errorMessage);
      }
      
      try {
        const data: ApiResponse<string> = await response.json();
        return data.data || 'Download started';
      } catch (err) {
        throw new Error('Invalid response from server');
      }
    }
  }

  static async cancelDownload(repoId: string): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('cancel_download', {
        modelId: repoId,
      });
    } else {
      const response = await apiFetch(`/models/download/cancel`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model_id: repoId }),
      });

      if (!response.ok) {
        let errorMessage = 'Failed to cancel download';
        try {
          const error: ApiResponse<any> = await response.json();
          errorMessage = error.error || errorMessage;
        } catch {
          errorMessage = response.statusText || errorMessage;
        }
        throw new Error(errorMessage);
      }

      const data: ApiResponse<string> = await response.json();
      return data.data || 'Download cancelled';
    }
  }

  static async searchModels(query: string, limit: number = 20): Promise<string> {
    if (isTauriApp) {
      // Desktop GUI: Use Tauri invoke
      return await invoke<string>('search_models', {
        query,
        limit,
        sort: "downloads",
        ggufOnly: true,
      });
    } else {
      // Web UI: Use REST API
      const response = await apiFetch(`/models/search?query=${encodeURIComponent(query)}&limit=${limit}`);
      if (!response.ok) {
        throw new Error(`Failed to search models: ${response.statusText}`);
      }
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Search completed';
    }
  }

  // Proxy operations
  static async getProxyStatus(): Promise<any> {
    if (isTauriApp) {
      return await invoke<any>('get_proxy_status');
    } else {
      const response = await apiFetch(`/proxy/status`);
      if (!response.ok) {
        throw new Error(`Failed to get proxy status: ${response.statusText}`);
      }
      const data: ApiResponse<any> = await response.json();
      return data.data || { running: false, port: 8080 };
    }
  }

  static async startProxy(config: {
    host: string;
    port: number;
    start_port: number;
    default_context: number;
  }): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('start_proxy', {
        host: config.host,
        port: config.port,
        startPort: config.start_port,
        defaultContext: config.default_context,
      });
    } else {
      const response = await apiFetch(`/proxy/start`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(config),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to start proxy');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Proxy started';
    }
  }

  static async stopProxy(): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('stop_proxy');
    } else {
      const response = await apiFetch(`/proxy/stop`, {
        method: 'POST',
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to stop proxy');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Proxy stopped';
    }
  }

  // Tag operations
  static async listTags(): Promise<string[]> {
    if (isTauriApp) {
      return await invoke<string[]>('list_tags');
    } else {
      const response = await apiFetch(`/tags`);
      if (!response.ok) {
        throw new Error(`Failed to fetch tags: ${response.statusText}`);
      }
      const data: ApiResponse<string[]> = await response.json();
      return data.data || [];
    }
  }

  static async addModelTag(modelId: number, tag: string): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('add_model_tag', { modelId, tag });
    } else {
      const response = await apiFetch(`/models/${modelId}/tags`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tag }),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to add tag to model');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Tag added to model successfully';
    }
  }

  static async removeModelTag(modelId: number, tag: string): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('remove_model_tag', { modelId, tag });
    } else {
      const response = await apiFetch(`/models/${modelId}/tags`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tag }),
      });
      
      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to remove tag from model');
      }
      
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Tag removed from model successfully';
    }
  }

  static async getModelTags(modelId: number): Promise<string[]> {
    if (isTauriApp) {
      return await invoke<string[]>('get_model_tags', { modelId });
    } else {
      const response = await apiFetch(`/models/${modelId}/tags`);
      if (!response.ok) {
        throw new Error(`Failed to fetch model tags: ${response.statusText}`);
      }
      const data: ApiResponse<string[]> = await response.json();
      return data.data || [];
    }
  }

  // Download Queue operations

  /**
   * Add a download to the queue. Returns the queue position and shard count.
   * Position 1 means will start immediately. Shard count > 1 indicates a sharded model.
   */
  static async queueDownload(modelId: string, quantization?: string): Promise<QueueDownloadResponse> {
    if (isTauriApp) {
      return await invoke<QueueDownloadResponse>('queue_download', { modelId, quantization });
    } else {
      const response = await apiFetch(`/models/download/queue`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model_id: modelId, quantization }),
      });

      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to queue download');
      }

      const data: ApiResponse<QueueDownloadResponse> = await response.json();
      return data.data || { position: 1, shard_count: 1 };
    }
  }

  /**
   * Get the current status of the download queue.
   */
  static async getDownloadQueue(): Promise<DownloadQueueStatus> {
    if (isTauriApp) {
      return await invoke<DownloadQueueStatus>('get_download_queue');
    } else {
      const response = await apiFetch(`/models/download/queue`);
      if (!response.ok) {
        throw new Error(`Failed to fetch download queue: ${response.statusText}`);
      }
      const data: ApiResponse<DownloadQueueStatus> = await response.json();
      return data.data || { pending: [], failed: [], max_size: 10 };
    }
  }

  /**
   * Remove a pending download from the queue.
   */
  static async removeFromDownloadQueue(modelId: string): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('remove_from_download_queue', { modelId });
    } else {
      const response = await apiFetch(`/models/download/queue/remove`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model_id: modelId }),
      });

      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to remove from queue');
      }

      const data: ApiResponse<string> = await response.json();
      return data.data || 'Removed from queue';
    }
  }

  /**
   * Cancel all shards in a shard group (for sharded model downloads).
   * This removes all pending shards and cancels any active download in the group.
   */
  static async cancelShardGroup(groupId: string): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('cancel_shard_group', { groupId });
    } else {
      const response = await apiFetch(`/models/download/queue/cancel-group`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ group_id: groupId }),
      });

      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to cancel shard group');
      }

      const data: ApiResponse<string> = await response.json();
      return data.data || 'Cancelled shard group';
    }
  }

  /**
   * Clear all failed downloads from the queue.
   */
  static async clearFailedDownloads(): Promise<string> {
    if (isTauriApp) {
      return await invoke<string>('clear_failed_downloads');
    } else {
      const response = await apiFetch(`/models/download/queue/clear-failed`, {
        method: 'POST',
      });

      if (!response.ok) {
        const error: ApiResponse<any> = await response.json();
        throw new Error(error.error || 'Failed to clear failed downloads');
      }

      const data: ApiResponse<string> = await response.json();
      return data.data || 'Cleared failed downloads';
    }
  }

  // ==========================================================================
  // Menu State Synchronization (Tauri desktop only)
  // ==========================================================================

  /**
   * Set the currently selected model ID to sync menu state.
   * This is only relevant for the Tauri desktop app.
   */
  static async setSelectedModel(modelId: number | null): Promise<void> {
    if (isTauriApp) {
      await invoke('set_selected_model', { modelId });
    }
    // No-op for web UI - menu sync is only needed for native menus
  }

  /**
   * Manually trigger a menu state sync.
   * Call this after actions that might affect menu state.
   */
  static async syncMenuState(): Promise<void> {
    if (isTauriApp) {
      await invoke('sync_menu_state');
    }
    // No-op for web UI
  }

  /**
   * Trigger menu state sync silently (swallowing errors).
   * Use this for fire-and-forget sync after state-changing operations.
   */
  static syncMenuStateSilent(): void {
    this.syncMenuState().catch(() => {
      // Silently ignore - menu sync is best-effort
    });
  }
}

