/**
 * Normalized Download Progress Types
 * 
 * These types provide a unified view of download progress for the UI layer,
 * normalizing both single-file DownloadProgress and multi-shard ShardProgress
 * events from the backend into a single, consistent shape.
 * 
 * This keeps UI components simple - they only need to understand one progress format.
 */

/**
 * Normalized download progress for UI consumption.
 * 
 * Represents overall progress for a model download, whether it's a single file
 * or a multi-shard model. The shard field is present only for multi-shard downloads.
 */
export interface NormalizedDownloadProgress {
  /** Canonical model ID (e.g., "unsloth/gpt-oss-120b-GGUF:Q4_K_M") */
  modelId: string;
  
  /** Total bytes to download (aggregate for multi-shard) */
  totalBytes: number;
  
  /** Bytes downloaded so far (aggregate for multi-shard) */
  downloadedBytes: number;
  
  /** Current download speed in bytes per second (null if not yet calculated) */
  speedBps: number | null;
  
  /** Estimated time remaining in seconds (null if not yet calculated) */
  etaSeconds: number | null;
  
  /** Shard information (present only for multi-shard downloads) */
  shard?: {
    /** Current shard index (1-based for display) */
    index: number;
    
    /** Total number of shards */
    count: number;
    
    /** Bytes downloaded for current shard only */
    shardDownloadedBytes: number;
    
    /** Total bytes for current shard only */
    shardTotalBytes: number;
  };
}

/**
 * Map a backend DownloadProgress event to normalized format.
 * Used for single-file downloads.
 */
export function normalizeDownloadProgress(event: {
  id: string;
  downloaded: number;
  total: number;
  speed_bps: number;
  eta_seconds: number;
  percentage: number;
}): NormalizedDownloadProgress {
  return {
    modelId: event.id,
    totalBytes: event.total,
    downloadedBytes: event.downloaded,
    speedBps: event.speed_bps,
    etaSeconds: event.eta_seconds,
    // No shard info for single-file downloads
  };
}

/**
 * Map a backend ShardProgress event to normalized format.
 * Used for multi-shard downloads.
 */
export function normalizeShardProgress(event: {
  id: string;
  shard_index: number;
  total_shards: number;
  shard_filename: string;
  shard_downloaded: number;
  shard_total: number;
  aggregate_downloaded: number;
  aggregate_total: number;
  speed_bps: number;
  eta_seconds: number;
  percentage: number;
}): NormalizedDownloadProgress {
  return {
    modelId: event.id,
    totalBytes: event.aggregate_total,
    downloadedBytes: event.aggregate_downloaded,
    speedBps: event.speed_bps,
    etaSeconds: event.eta_seconds,
    shard: {
      index: event.shard_index + 1, // Convert 0-based to 1-based for display
      count: event.total_shards,
      shardDownloadedBytes: event.shard_downloaded,
      shardTotalBytes: event.shard_total,
    },
  };
}
