/**
 * Downloads API module.
 * Handles download queue management for HuggingFace models.
 */

import { get, post, del } from './client';
import type { DownloadId } from '../types/ids';
import type {
  DownloadQueueStatus,
  DownloadQueueItem,
  QueueDownloadParams,
  QueueDownloadResponse,
} from '../types/downloads';

/**
 * Raw backend response shape for queue snapshot.
 * Backend returns a flat list of all items that we need to split.
 */
interface QueueSnapshotResponse {
  items: DownloadQueueItem[];
  max_size: number;
  active_count: number;
  pending_count: number;
}

/**
 * Get current download queue status.
 * Transforms the backend's flat item list into categorized current/pending/failed.
 */
export async function getDownloadQueue(): Promise<DownloadQueueStatus> {
  const snapshot = await get<QueueSnapshotResponse>('/api/downloads/queue');
  
  // Split items by status (same logic as SSE event handler)
  const items = snapshot.items || [];
  const current = items.find((item) => item.status === 'downloading') ?? null;
  const pending = items.filter((item) => item.status === 'queued');
  const failed = items.filter((item) => item.status === 'failed');
  
  return {
    current,
    pending,
    failed,
    max_size: snapshot.max_size,
  };
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
  await post<void>('/api/downloads/reorder-full', { ids });
}

/**
 * Reorder a single download to a specific position.
 * @param id - Download ID to reorder
 * @param position - Target 1-based position in queue
 * @returns Actual position after reorder
 */
export async function reorderQueueItem(id: DownloadId, position: number): Promise<number> {
  const response = await post<number>('/api/downloads/reorder', {
    model_id: id,
    position,
  });
  return response;
}
