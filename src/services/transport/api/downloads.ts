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
import type { QueueDownloadRequest } from '../../../types/generated/QueueDownloadRequest';
import type { ReorderFullRequest } from '../../../types/generated/ReorderFullRequest';
import type { ReorderRequest } from '../../../types/generated/ReorderRequest';

/**
 * Get the download queue: the same snapshot a `queue_snapshot` event carries.
 */
export async function getDownloadQueue(): Promise<QueueSnapshot> {
  return get<QueueSnapshot>('/api/models/downloads/queue');
}

/**
 * Queue a new download from HuggingFace. The answer is the download's id:
 * its row's in the queue snapshot.
 */
export async function queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
  // An absent `quantization` is `None` to the daemon: it chooses one.
  const body: QueueDownloadRequest = { model_id: params.modelId, quantization: params.quantization };
  return post<QueueDownloadResponse>('/api/models/downloads/queue', body);
}

/**
 * Cancel a download that is waiting or running, every file of it. It ends
 * with a cancelled outcome, unless the cancel came too late: its last file was
 * already on disk and its model being registered, or a file of it had already
 * failed. Then it ends completed or failed, as it was going to.
 */
export async function cancelDownload(id: DownloadId): Promise<void> {
  await post<void>(`/api/models/downloads/${encodeURIComponent(id)}/cancel`);
}

/**
 * Take a download off the queue. One that is waiting or running is
 * cancelled; one that has ended has its entry dropped from `finished`.
 */
export async function removeFromQueue(id: DownloadId): Promise<void> {
  await del<void>(`/api/models/downloads/${encodeURIComponent(id)}`);
}

/**
 * Reorder downloads in the queue.
 * @param ids - The waiting downloads in the order wanted, one id per download.
 *   Each is moved to its index + 1. That is its snapshot position only while
 *   nothing is running; behind a running download the waiting places start at
 *   2, so the order that results can differ from the one given.
 */
export async function reorderQueue(ids: DownloadId[]): Promise<void> {
  const body: ReorderFullRequest = { ids };
  await post<void>('/api/models/downloads/reorder-full', body);
}

/**
 * Reorder a single download to a specific position.
 * @param id - Download ID to reorder
 * @param position - Target 1-based position, counted in downloads as the
 *   snapshot's `position` is: the running download is 1, the first waiting one 2
 * @returns Actual position after reorder
 */
export async function reorderQueueItem(id: DownloadId, position: number): Promise<number> {
  const body: ReorderRequest = { model_id: id, position };
  return post<number>('/api/models/downloads/reorder', body);
}
