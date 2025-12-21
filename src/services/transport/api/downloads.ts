/**
 * Downloads API module.
 * Handles download queue management for HuggingFace models.
 */

import { get, post, del } from './client';
import type { DownloadId } from '../types/ids';
import type {
  DownloadQueueStatus,
  QueueDownloadParams,
  QueueDownloadResponse,
} from '../types/downloads';

/**
 * Get current download queue status.
 */
export async function getDownloadQueue(): Promise<DownloadQueueStatus> {
  return get<DownloadQueueStatus>('/api/downloads/queue');
}

/**
 * Queue a new download from HuggingFace.
 */
export async function queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
  return post<QueueDownloadResponse>('/api/downloads/queue', {
    model_id: params.modelId,
    quantization: params.quantization,
    target_path: params.targetPath,
  });
}

/**
 * Cancel an active or queued download.
 */
export async function cancelDownload(id: DownloadId): Promise<void> {
  await post<void>(`/api/downloads/${encodeURIComponent(id)}/cancel`);
}

/**
 * Remove a download from the queue (for failed/completed items).
 */
export async function removeFromQueue(id: DownloadId): Promise<void> {
  await del<void>(`/api/downloads/${encodeURIComponent(id)}`);
}

/**
 * Clear all failed downloads from the queue.
 */
export async function clearFailedDownloads(): Promise<void> {
  await post<void>('/api/downloads/failed/clear');
}

/**
 * Cancel all shards in a download group.
 */
export async function cancelShardGroup(groupId: string): Promise<void> {
  await post<void>(`/api/downloads/shard-group/${encodeURIComponent(groupId)}/cancel`);
}

/**
 * Reorder downloads in the queue.
 */
export async function reorderQueue(ids: DownloadId[]): Promise<void> {
  await post<void>('/api/downloads/reorder', { ids });
}
