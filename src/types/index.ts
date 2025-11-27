export interface GgufModel {
  id?: number;
  name: string;
  file_path: string;
  param_count_b: number;
  architecture?: string;
  quantization?: string;
  context_length?: number;
  added_at: string;
  hf_repo_id?: string;
  tags?: string[];
  // Server status
  is_serving?: boolean;
  port?: number;
}

export interface DownloadConfig {
  repo_id: string;
  quantization?: string;
}

export interface ServeConfig {
  id: number;
  ctx_size?: string;
  context_length?: number;
  mlock?: boolean;
  port?: number;
  jinja?: boolean;
}

export interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  status: string;
}

export interface ModelsDirectoryInfo {
  path: string;
  source: 'explicit' | 'env' | 'default';
  default_path: string;
  exists: boolean;
  writable: boolean;
}

export interface AppSettings {
  default_download_path?: string | null;
  default_context_size?: number | null;
  proxy_port?: number | null;
  server_port?: number | null;
  max_download_queue_size?: number | null;
}

export interface UpdateSettingsRequest {
  default_download_path?: string | null | undefined;
  default_context_size?: number | null | undefined;
  proxy_port?: number | null | undefined;
  server_port?: number | null | undefined;
  max_download_queue_size?: number | null | undefined;
}

// Download Queue Types

export type DownloadStatus = 'downloading' | 'queued' | 'completed' | 'failed';

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
}

export interface DownloadQueueItem {
  model_id: string;
  quantization?: string | null;
  status: DownloadStatus;
  position: number;
  error?: string | null;
  /** Shared identifier for all shards of a sharded model download */
  group_id?: string | null;
  /** Shard information if this item is part of a sharded model download */
  shard_info?: ShardInfo | null;
}

export interface DownloadQueueStatus {
  current?: DownloadQueueItem | null;
  pending: DownloadQueueItem[];
  failed: DownloadQueueItem[];
  max_size: number;
}
