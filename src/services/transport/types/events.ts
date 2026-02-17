/**
 * Events transport sub-interface.
 * Handles real-time event subscriptions.
 */

import type { Unsubscribe, EventHandler } from './common';
import type { DownloadId } from './ids';

// ============================================================================
// Server Events
// ============================================================================

export type ServerStatus = 'running' | 'stopping' | 'stopped' | 'crashed';

export interface ServerStateInfo {
  modelId: string;
  status: ServerStatus;
  port?: number;
  updatedAt: number;
}

export type ServerEvent =
  | { type: 'snapshot'; servers: ServerStateInfo[] }
  | { type: 'running'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopping'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopped'; modelId: string; port?: number; updatedAt: number }
  | { type: 'crashed'; modelId: string; port?: number; updatedAt: number };

// ============================================================================
// Download Events
// ============================================================================

export interface DownloadSummary {
  id: DownloadId;
  display_name: string;
  status: 'queued' | 'downloading' | 'completed' | 'failed' | 'cancelled';
  position: number;
  error?: string | null;
  group_id?: string | null;
  shard_info?: {
    shard_index: number;
    total_shards: number;
    filename: string;
    file_size?: number | null;
  } | null;
}

/**
 * Stable artifact identity for completion tracking.
 * Represents "what the user thinks they downloaded" from an artifact perspective.
 */
export type CompletionKey =
  | {
      kind: 'hf_file';
      repo_id: string;
      revision: string;
      filename_canon: string;
      quantization?: string;
    }
  | {
      kind: 'url_file';
      url: string;
      filename: string;
    }
  | {
      kind: 'local_file';
      path: string;
    };

/**
 * Breakdown of attempts by result kind.
 */
export interface AttemptCounts {
  downloaded: number;
  failed: number;
  cancelled: number;
}

/**
 * Result kind for a completion attempt.
 */
export type CompletionKind = 'downloaded' | 'failed' | 'cancelled' | 'already_present';

/**
 * Details for a single completed artifact in a queue run.
 */
export interface CompletionDetail {
  key: CompletionKey;
  display_name: string;
  last_result: CompletionKind;
  last_completed_at_ms: number;
  download_ids: DownloadId[];
  attempt_counts: AttemptCounts;
}

/**
 * Summary of an entire queue run from start to drain.
 * Emitted when the queue transitions from busy → idle.
 */
export interface QueueRunSummary {
  run_id: string;
  started_at_ms: number;
  completed_at_ms: number;
  total_attempts_downloaded: number;
  total_attempts_failed: number;
  total_attempts_cancelled: number;
  unique_models_downloaded: number;
  unique_models_failed: number;
  unique_models_cancelled: number;
  truncated: boolean;
  items: CompletionDetail[];
}

export type DownloadEvent =
  | { type: 'queue_snapshot'; items: DownloadSummary[]; max_size: number }
  | { type: 'download_started'; id: DownloadId; shard_index?: number; total_shards?: number }
  | { type: 'download_progress'; id: DownloadId; downloaded: number; total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'shard_progress'; id: DownloadId; shard_index: number; total_shards: number; shard_filename: string; shard_downloaded: number; shard_total: number; aggregate_downloaded: number; aggregate_total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'download_completed'; id: DownloadId; message?: string | null }
  | { type: 'download_failed'; id: DownloadId; error: string }
  | { type: 'download_cancelled'; id: DownloadId }
  | { type: 'queue_run_complete'; summary: QueueRunSummary };

// ============================================================================
// Log Events
// ============================================================================

export interface LogEntry {
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  source?: string;
}

export type LogEvent = LogEntry;

// ============================================================================
// Verification Events
// ============================================================================

export type OverallHealth = 'healthy' | 'unhealthy' | 'unverifiable';

export interface VerificationProgressEvent {
  type: 'verification_progress';
  modelId: number;
  modelName: string;
  shardName: string;
  bytesProcessed: number;
  totalBytes: number;
}

export interface VerificationCompleteEvent {
  type: 'verification_complete';
  modelId: number;
  modelName: string;
  overallHealth: OverallHealth;
}

export type VerificationEvent = VerificationProgressEvent | VerificationCompleteEvent;

// ============================================================================
// App Event Map
// ============================================================================

/**
 * Map of all event types to their payload types.
 * Used for type-safe event subscriptions.
 *
 * Note: Download events arrive wrapped as { type: "download", event: DownloadEvent }
 * to preserve all details including shard progress.
 */
export interface AppEventMap {
  'server': ServerEvent;
  'download': { type: 'download'; event: DownloadEvent };
  'log': LogEvent;
  'verification': VerificationEvent;
}

export type AppEventType = keyof AppEventMap;

// ============================================================================
// Events Transport Interface
// ============================================================================

/**
 * Events transport operations.
 */
export interface EventsTransport {
  /**
   * Subscribe to an event stream.
   * Returns an unsubscribe function.
   */
  subscribe<K extends AppEventType>(
    event: K,
    handler: EventHandler<AppEventMap[K]>
  ): Unsubscribe;
}
