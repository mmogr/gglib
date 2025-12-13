/**
 * Downloads transport sub-interface.
 * Handles download queue management.
 */

import type { DownloadId, HfModelId } from './ids';

/**
 * Download status.
 */
export type DownloadStatus = 'queued' | 'downloading' | 'completed' | 'failed' | 'cancelled';

/**
 * Shard information for multi-file downloads.
 */
export interface ShardInfo {
  shard_index: number;
  total_shards: number;
  filename: string;
  file_size?: number | null;
}

/**
 * Download queue item.
 */
export interface DownloadQueueItem {
  id: DownloadId;
  display_name: string;
  status: DownloadStatus;
  position: number;
  error?: string | null;
  group_id?: string | null;
  shard_info?: ShardInfo | null;
}

/**
 * Download queue status.
 */
export interface DownloadQueueStatus {
  current?: DownloadQueueItem | null;
  pending: DownloadQueueItem[];
  failed: DownloadQueueItem[];
  max_size: number;
}

/**
 * Parameters for queueing a download.
 */
export interface QueueDownloadParams {
  modelId: HfModelId;
  /** Optional quantization. If omitted, smart selection picks the best available. */
  quantization?: string;
  targetPath?: string;
}

/**
 * Response from queueing a download.
 * Canonical shape returned by all transports.
 */
export interface QueueDownloadResponse {
  /** Position in the queue (0 = downloading now). */
  position: number;
  /** Number of shards queued (1 for single file, N for sharded models). */
  shard_count: number;
}

/**
 * Typed payload for download completion events.
 * Used by the UI effects layer (useDownloadCompletionEffects) to trigger
 * model refresh and toast notifications.
 */
export interface DownloadCompletionInfo {
  /** Canonical download ID (model_id:quantization or model_id) */
  modelId: string;
  /** Quantization variant if applicable (e.g., "Q4_K_M") */
  quantization?: string;
  /** Human-readable display name for toast messages */
  displayName?: string;
  /** Source of the download for potential future handling differentiation */
  source: 'huggingface' | 'local' | 'unknown';
}

/**
 * Downloads transport operations.
 */
export interface DownloadsTransport {
  /** Get current download queue status. */
  getDownloadQueue(): Promise<DownloadQueueStatus>;

  /** Queue a new download from HuggingFace. */
  queueDownload(params: QueueDownloadParams): Promise<QueueDownloadResponse>;

  /** Cancel an active or queued download. */
  cancelDownload(id: DownloadId): Promise<void>;

  /** Remove a download from the queue (for failed/completed items). */
  removeFromQueue(id: DownloadId): Promise<void>;

  /** Clear all failed downloads from the queue. */
  clearFailedDownloads(): Promise<void>;

  /** Cancel all shards in a download group. */
  cancelShardGroup(groupId: string): Promise<void>;

  /** Reorder downloads in the queue. */
  reorderQueue(ids: DownloadId[]): Promise<void>;
}
