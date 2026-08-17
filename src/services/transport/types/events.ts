/**
 * Events transport sub-interface.
 * Handles real-time event subscriptions.
 */

import type { Unsubscribe, EventHandler } from './common';
import type { DownloadId } from './ids';
import type { RuntimeErrorInfo, ServerHealthStatus } from '../../../types';

// ============================================================================
// Server Events
// ============================================================================

/**
 * One running server in a `server_snapshot` frame.
 *
 * Mirrors `gglib_core::events::ServerSnapshotEntry` (camelCase). `startedAt`
 * is Unix **seconds**, unlike the millisecond `timestamp` on
 * `server_health_changed` — the normalizer converts.
 */
export interface ServerSnapshotEntry {
  modelId: number;
  modelName: string;
  port: number;
  startedAt: number;
  healthy: boolean;
}

/**
 * The server frames `/api/events` actually carries.
 *
 * These are `AppEvent`'s server variants as serde writes them — snake_case
 * tags, camelCase fields, `modelId` a *number*. Deliberately not the shape
 * consumers work with: `serverEvents.normalize` turns these into the
 * registry's `ServerEvent`, which is keyed by string and speaks in
 * running/stopping/stopped/crashed. Two shapes, because there really are two.
 */
export type ServerWireEvent =
  | { type: 'server_started'; modelId: number; modelName: string; port: number }
  | { type: 'server_stopped'; modelId: number; modelName: string }
  // `modelId` is null when the runtime could not attribute the failure, and is
  // always present — the Rust field carries no `skip_serializing_if`.
  | { type: 'server_error'; modelId: number | null; modelName: string; error: RuntimeErrorInfo }
  | { type: 'server_snapshot'; servers: ServerSnapshotEntry[] }
  | {
      type: 'server_health_changed';
      serverId: number;
      modelId: number;
      // Nested, not flat: `ServerHealthStatus` is internally tagged on `status`
      // and the field is also named `status`, so the wire reads
      // `"status": { "status": "healthy" }`.
      status: ServerHealthStatus;
      detail?: string;
      /** Unix milliseconds. */
      timestamp: number;
    };

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
  // speed_bps / eta_seconds are omitted while the manager's rate estimator is
  // still warming up. Absent means unknown, never zero — render a placeholder.
  | { type: 'download_progress'; id: DownloadId; downloaded: number; total: number; speed_bps?: number; eta_seconds?: number; percentage: number }
  | { type: 'shard_progress'; id: DownloadId; shard_index: number; total_shards: number; shard_filename: string; shard_downloaded: number; shard_total: number; aggregate_downloaded: number; aggregate_total: number; speed_bps?: number; eta_seconds?: number; percentage: number }
  | { type: 'download_completed'; id: DownloadId; message?: string | null }
  | { type: 'download_failed'; id: DownloadId; error: string }
  | { type: 'download_cancelled'; id: DownloadId }
  | { type: 'download_status_changed'; id: DownloadId; status: import('./downloads').DownloadStatus }
  // Transient, non-persisted note about work happening for this download
  // that produces no byte progress (e.g. first-run Python env setup for the
  // fast downloader). Unlike download_status_changed, message is free-form
  // text rather than a fixed status.
  | { type: 'download_notice'; id: DownloadId; message: string }
  | { type: 'queue_run_complete'; summary: QueueRunSummary };

// ============================================================================
// Model Events
// ============================================================================

/**
 * The lightweight model shape library events carry — deliberately not the full
 * model row, since a listener's job is to notice the change, not to render
 * from the notification.
 *
 * Mirrors `gglib_core::events::ModelSummary` (camelCase).
 */
export interface ModelEventSummary {
  id: number;
  name: string;
  filePath: string;
  architecture?: string;
  quantization?: string;
}

/**
 * Library changes, broadcast to every client attached to the daemon.
 *
 * These let a second window or browser tab reach a list that would otherwise
 * refresh only when its own tab made the edit. A `gglib model add` in a
 * terminal is a separate process and does not reach here.
 */
export type ModelEvent =
  | { type: 'model_added'; model: ModelEventSummary }
  | { type: 'model_updated'; model: ModelEventSummary }
  | { type: 'model_removed'; modelId: number };

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
// Proxy Events
// ============================================================================

export type ProxyEvent =
  | { type: 'proxy_started'; port: number }
  | { type: 'proxy_stopped' }
  | { type: 'proxy_crashed' };

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
  'server': ServerWireEvent;
  'download': { type: 'download'; event: DownloadEvent };
  'model': ModelEvent;
  'verification': VerificationEvent;
  'proxy': ProxyEvent;
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
