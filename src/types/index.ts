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
  title_generation_prompt?: string | null;
  show_memory_fit_indicators?: boolean | null;
  /** Maximum iterations for tool calling agentic loop (default: 10) */
  max_tool_iterations?: number | null;
}

export interface UpdateSettingsRequest {
  default_download_path?: string | null | undefined;
  default_context_size?: number | null | undefined;
  proxy_port?: number | null | undefined;
  server_port?: number | null | undefined;
  max_download_queue_size?: number | null | undefined;
  title_generation_prompt?: string | null | undefined;
  show_memory_fit_indicators?: boolean | null | undefined;
  /** Maximum iterations for tool calling agentic loop (default: 10) */
  max_tool_iterations?: number | null | undefined;
}

// ============================================================================
// System Memory Types (for "Will it fit?" indicators)
// ============================================================================

/**
 * System memory information for model fit calculations.
 */
export interface SystemMemoryInfo {
  /** Total system RAM in bytes */
  total_ram_bytes: number;
  /** GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon) */
  gpu_memory_bytes?: number | null;
  /** Whether the system has Apple Silicon with unified memory */
  is_apple_silicon: boolean;
  /** Whether the system has an NVIDIA GPU */
  has_nvidia_gpu: boolean;
}

/**
 * Fit status for a model quantization based on available memory.
 */
export type FitStatus = 'fits' | 'tight' | 'wont_fit';

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
  /** Size of this shard in bytes (if available from HuggingFace API) */
  file_size?: number | null;
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

// ============================================================================
// HuggingFace Browser Types
// ============================================================================

/**
 * Summary of a HuggingFace model from the search API.
 */
export interface HfModelSummary {
  /** Model ID (e.g., "TheBloke/Llama-2-7B-GGUF") */
  id: string;
  /** Human-readable model name (derived from id) */
  name: string;
  /** Author/organization (e.g., "TheBloke") */
  author?: string | null;
  /** Total download count */
  downloads: number;
  /** Like count */
  likes: number;
  /** Last modified timestamp */
  last_modified?: string | null;
  /** Total parameter count in billions (from safetensors.total) */
  parameters_b?: number | null;
  /** Model description/README excerpt */
  description?: string | null;
  /** Model tags */
  tags: string[];
}

/**
 * Sort field options for HuggingFace model search.
 */
export type HfSortField = 'downloads' | 'likes' | 'modified' | 'created' | 'id';

/**
 * Request for searching HuggingFace models.
 */
export interface HfSearchRequest {
  /** Search query (model name) */
  query?: string | null;
  /** Minimum parameters in billions */
  min_params_b?: number | null;
  /** Maximum parameters in billions */
  max_params_b?: number | null;
  /** Page number (0-indexed) */
  page: number;
  /** Results per page (default 30) */
  limit: number;
  /** Sort field (default: downloads) */
  sort_by?: HfSortField;
  /** Sort direction: true = ascending, false = descending (default: false/descending) */
  sort_ascending?: boolean;
}

/**
 * Response from HuggingFace model search.
 */
export interface HfSearchResponse {
  /** Models matching the search criteria */
  models: HfModelSummary[];
  /** Whether more results are available */
  has_more: boolean;
  /** Current page number (0-indexed) */
  page: number;
  /** Total count of matching models (if available) */
  total_count?: number | null;
}

/**
 * Information about a specific quantization variant.
 */
export interface HfQuantization {
  /** Quantization name (e.g., "Q4_K_M", "Q8_0") */
  name: string;
  /** File path within the repository */
  file_path: string;
  /** File size in bytes */
  size_bytes: number;
  /** File size in MB (for display) */
  size_mb: number;
  /** Whether this is a sharded model (multiple files) */
  is_sharded: boolean;
  /** Number of shards if sharded */
  shard_count?: number | null;
}

/**
 * Response containing available quantizations for a model.
 */
export interface HfQuantizationsResponse {
  /** Model ID */
  model_id: string;
  /** Available quantizations */
  quantizations: HfQuantization[];
}

// ============================================================================
// Model Filter Options Types
// ============================================================================

/**
 * A range of numeric values with min and max.
 */
export interface RangeValues {
  min: number;
  max: number;
}

/**
 * Filter options for the model library UI.
 * Contains aggregate data about available models for building dynamic filter controls.
 */
export interface ModelFilterOptions {
  /** All distinct quantization types present in the library */
  quantizations: string[];
  /** Minimum and maximum parameter counts (in billions) */
  param_range: RangeValues | null;
  /** Minimum and maximum context lengths */
  context_range: RangeValues | null;
}
