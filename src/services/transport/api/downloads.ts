/**
 * Downloads API module.
 * Handles download queue management for HuggingFace models.
 */

import { get, post, del } from './client';
import type { DownloadId } from '../types/ids';
import type {
  QueueSnapshot,
  QueueDownloadParams,
  QueueDownloadResponse,
} from '../types/downloads';

/**
 * Get the download queue: the same snapshot a `queue_snapshot` event carries.
 */
export async function getDownloadQueue(): Promise<QueueSnapshot> {
  return get<QueueSnapshot>('/api/models/downloads/queue');
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
 * Reorder downloads in the queue.
 * @param ids - The waiting downloads in the order wanted, one id per download.
 *   Each is moved to its index + 1. That is its snapshot position only while
 *   nothing is running; behind a running download the waiting places start at
 *   2, so the order that results can differ from the one given.
 */
export async function reorderQueue(ids: DownloadId[]): Promise<void> {
  await post<void>('/api/models/downloads/reorder-full', { ids });
}

/**
 * Reorder a single download to a specific position.
 * @param id - Download ID to reorder
 * @param position - Target 1-based position, counted in downloads as the
 *   snapshot's `position` is: the running download is 1, the first waiting one 2
 * @returns Actual position after reorder
 */
export async function reorderQueueItem(id: DownloadId, position: number): Promise<number> {
  const response = await post<number>('/api/models/downloads/reorder', {
    model_id: id,
    position,
  });
  return response;
}
