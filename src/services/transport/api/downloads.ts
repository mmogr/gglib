/**
 * Downloads API module.
 * Handles download queue management for HuggingFace models.
 */

import { get, post, del } from './client';
import { bucketQueue } from '../downloadQueue';
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
  const snapshot = await get<QueueSnapshotResponse>('/api/models/downloads/queue');
  
  // Bucketed by the same function the SSE `queue_snapshot` path uses. The
  // comment here used to claim that and it was not true: the SSE side ran
  // every row through a status map first, so the two produced different
  // queues from the same data depending on which answered last.
  return bucketQueue(snapshot.items || [], snapshot.max_size);
}

/**
 * Queue a new download from HuggingFace.
 */
export async function queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
  return post<QueueDownloadResponse>('/api/models/downloads/queue', {
    model_id: params.modelId,
    quantization: params.quantization,
    target_path: params.targetPath,
  });
}

/**
 * Cancel an active or queued download.
 */
export async function cancelDownload(id: DownloadId): Promise<void> {
  await post<void>(`/api/models/downloads/${encodeURIComponent(id)}/cancel`);
}

/**
 * Remove a download from the queue (for failed/completed items).
 */
export async function removeFromQueue(id: DownloadId): Promise<void> {
  await del<void>(`/api/models/downloads/${encodeURIComponent(id)}`);
}

/**
 * Clear all failed downloads from the queue.
 */
export async function clearFailedDownloads(): Promise<void> {
  await post<void>('/api/models/downloads/failed/clear');
}

/**
 * Cancel all shards in a download group.
 */
export async function cancelShardGroup(groupId: string): Promise<void> {
  await post<void>(`/api/models/downloads/shard-group/${encodeURIComponent(groupId)}/cancel`);
}

/**
 * Reorder downloads in the queue.
 */
export async function reorderQueue(ids: DownloadId[]): Promise<void> {
  await post<void>('/api/models/downloads/reorder-full', { ids });
}

/**
 * Reorder a single download to a specific position.
 * @param id - Download ID to reorder
 * @param position - Target 1-based position in queue
 * @returns Actual position after reorder
 */
export async function reorderQueueItem(id: DownloadId, position: number): Promise<number> {
  const response = await post<number>('/api/models/downloads/reorder', {
    model_id: id,
    position,
  });
  return response;
}
