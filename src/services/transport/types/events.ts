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

export type DownloadEvent =
  | { type: 'queue_snapshot'; items: DownloadSummary[]; max_size: number }
  | { type: 'download_started'; id: DownloadId }
  | { type: 'download_progress'; id: DownloadId; downloaded: number; total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'shard_progress'; id: DownloadId; shard_index: number; total_shards: number; shard_filename: string; shard_downloaded: number; shard_total: number; aggregate_downloaded: number; aggregate_total: number; speed_bps: number; eta_seconds: number; percentage: number }
  | { type: 'download_completed'; id: DownloadId; message?: string | null }
  | { type: 'download_failed'; id: DownloadId; error: string }
  | { type: 'download_cancelled'; id: DownloadId };

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
