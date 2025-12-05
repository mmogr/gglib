// Download Domain Types
// Canonical source for all download-related TypeScript types

// ============================================================================
// Core Identifiers
// ============================================================================

/** Canonical download identifier (model_id:quantization or model_id) */
export type DownloadId = string;

/** Status of a download in the queue */
export type DownloadStatus = 'downloading' | 'queued' | 'completed' | 'failed';

// ============================================================================
// Shard Types
// ============================================================================

/**
 * Information about a shard in a sharded model download.
 * Sharded models are split across multiple files that must be downloaded together.
 */
export interface ShardInfo {
  /** Zero-based index of this shard (0, 1, 2, ...) */
  shard_index: number;
  /** Total number of shards in the group */
  total_shards: number;
  /** Filename of this specific shard (e.g., "Q4_K_M/model-00001-of-00003.gguf") */
  filename: string;
  /** Size of this shard in bytes (if available from HuggingFace API) */
  file_size?: number | null;
}

// ============================================================================
// Queue Types
// ============================================================================

/**
 * Summary of a download in the queue (from QueueSnapshot events).
 */
export interface DownloadSummary {
  id: string;
  display_name: string;
  status: 'queued' | 'downloading' | 'completed' | 'failed' | 'cancelled';
  position: number;
  error?: string | null;
  group_id?: string | null;
  shard_info?: ShardInfo | null;
}

/**
 * Queue item for frontend display (normalized from DownloadSummary).
 */
export interface DownloadQueueItem {
  /** Canonical ID string (model_id:quantization or just model_id) */
  id: string;
  /** Human-readable display name */
  display_name: string;
  status: DownloadStatus;
  position: number;
  error?: string | null;
  /** Shared identifier for all shards of a sharded model download */
  group_id?: string | null;
  /** Shard information if this item is part of a sharded model download */
  shard_info?: ShardInfo | null;
}

/**
 * Current state of the download queue.
 */
export interface DownloadQueueStatus {
  current?: DownloadQueueItem | null;
  pending: DownloadQueueItem[];
  failed: DownloadQueueItem[];
  max_size: number;
}

// ============================================================================
// Event Types (SSE events from DownloadManager)
// ============================================================================

/**
 * Discriminated union of all download events from the backend.
 * Received via SSE at /api/models/download/progress or Tauri events.
 */
export type DownloadEvent =
  | { type: 'queue_snapshot'; items: DownloadSummary[]; max_size: number }
  | { type: 'download_started'; id: string }
  | { type: 'download_progress'; id: string; downloaded: number; total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'shard_progress'; id: string; shard_index: number; total_shards: number; shard_filename: string; shard_downloaded: number; shard_total: number; aggregate_downloaded: number; aggregate_total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'download_completed'; id: string; message?: string | null }
  | { type: 'download_failed'; id: string; error: string }
  | { type: 'download_cancelled'; id: string };

// ============================================================================
// Utility Types
// ============================================================================

/** Progress map keyed by DownloadId for easy lookup in the UI */
export type ProgressById = Map<DownloadId, DownloadEvent>;

// ============================================================================
// Completion Info (for UI effects layer)
// ============================================================================

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
