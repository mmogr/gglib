/**
 * Downloads client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/downloads
 */

import { getTransport } from '../transport';
import type { DownloadId } from '../transport/types/ids';
import type {
  DownloadQueueStatus,
  QueueDownloadParams,
  QueueDownloadResponse,
} from '../transport/types/downloads';

/**
 * Get current download queue status.
 */
export async function getDownloadQueue(): Promise<DownloadQueueStatus> {
  return getTransport().getDownloadQueue();
}

/**
 * Queue a new download from HuggingFace.
 */
export async function queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse> {
  return getTransport().queueDownload(params);
}

/**
 * Cancel an active or queued download.
 */
export async function cancelDownload(id: DownloadId): Promise<void> {
  return getTransport().cancelDownload(id);
}

/**
 * Remove a download from the queue (for failed/completed items).
 */
export async function removeFromQueue(id: DownloadId): Promise<void> {
  return getTransport().removeFromQueue(id);
}

/**
 * Clear all failed downloads from the queue.
 */
export async function clearFailedDownloads(): Promise<void> {
  return getTransport().clearFailedDownloads();
}

/**
 * Cancel all shards in a download group.
 */
export async function cancelShardGroup(groupId: string): Promise<void> {
  return getTransport().cancelShardGroup(groupId);
}

/**
 * Reorder downloads in the queue.
 */
export async function reorderQueue(ids: DownloadId[]): Promise<void> {
  return getTransport().reorderQueue(ids);
}
