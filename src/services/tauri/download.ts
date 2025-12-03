// Download domain operations
// Download models from HuggingFace, queue management

import { DownloadConfig, DownloadQueueStatus } from "../../types";
import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

/**
 * Response from queue_download containing position and shard count.
 */
export interface QueueDownloadResponse {
  position: number;
  shard_count: number;
}

/**
 * Start downloading a model from HuggingFace.
 */
export async function downloadModel(config: DownloadConfig): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('download_model', {
      modelId: config.repo_id,
      quantization: config.quantization,
    });
  } else {
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
        const error: ApiResponse<unknown> = await response.json();
        errorMessage = (error.error as string) || errorMessage;
      } catch {
        errorMessage = response.statusText || errorMessage;
      }
      throw new Error(errorMessage);
    }

    try {
      const data: ApiResponse<string> = await response.json();
      return data.data || 'Download started';
    } catch {
      throw new Error('Invalid response from server');
    }
  }
}

/**
 * Cancel an active download.
 */
export async function cancelDownload(repoId: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('cancel_download', {
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
        const error: ApiResponse<unknown> = await response.json();
        errorMessage = (error.error as string) || errorMessage;
      } catch {
        errorMessage = response.statusText || errorMessage;
      }
      throw new Error(errorMessage);
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Download cancelled';
  }
}

/**
 * Add a download to the queue. Returns the queue position and shard count.
 * Position 1 means will start immediately. Shard count > 1 indicates a sharded model.
 */
export async function queueDownload(
  modelId: string,
  quantization?: string
): Promise<QueueDownloadResponse> {
  if (isTauriApp) {
    return await tauriInvoke<QueueDownloadResponse>('queue_download', { modelId, quantization });
  } else {
    const response = await apiFetch(`/models/download/queue`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model_id: modelId, quantization }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to queue download');
    }

    const data: ApiResponse<QueueDownloadResponse> = await response.json();
    return data.data || { position: 1, shard_count: 1 };
  }
}

/**
 * Get the current status of the download queue.
 */
export async function getDownloadQueue(): Promise<DownloadQueueStatus> {
  if (isTauriApp) {
    return await tauriInvoke<DownloadQueueStatus>('get_download_queue');
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
export async function removeFromDownloadQueue(modelId: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('remove_from_download_queue', { modelId });
  } else {
    const response = await apiFetch(`/models/download/queue/remove`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model_id: modelId }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to remove from queue');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Removed from queue';
  }
}

/**
 * Reorder a pending download to a new position in the queue.
 * For sharded models, all shards are moved together as a unit.
 *
 * @param modelId - The model ID to move
 * @param newPosition - The target position (0-based index)
 * @returns The actual position where the item(s) were placed
 */
export async function reorderDownloadQueue(
  modelId: string,
  newPosition: number
): Promise<number> {
  if (isTauriApp) {
    return await tauriInvoke<number>('reorder_download_queue', { modelId, newPosition });
  } else {
    const response = await apiFetch(`/models/download/queue/reorder`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model_id: modelId, new_position: newPosition }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to reorder queue');
    }

    const data: ApiResponse<{ actual_position: number }> = await response.json();
    return data.data?.actual_position ?? newPosition;
  }
}

/**
 * Cancel all shards in a shard group (for sharded model downloads).
 * This removes all pending shards and cancels any active download in the group.
 */
export async function cancelShardGroup(groupId: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('cancel_shard_group', { groupId });
  } else {
    const response = await apiFetch(`/models/download/queue/cancel-group`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ group_id: groupId }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to cancel shard group');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Cancelled shard group';
  }
}

/**
 * Clear all failed downloads from the queue.
 */
export async function clearFailedDownloads(): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('clear_failed_downloads');
  } else {
    const response = await apiFetch(`/models/download/queue/clear-failed`, {
      method: 'POST',
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to clear failed downloads');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Cleared failed downloads';
  }
}
