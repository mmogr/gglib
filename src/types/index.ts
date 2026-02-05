// ============================================================================
// Inference Configuration
// ============================================================================

/**
 * Inference parameters for LLM sampling.
 * All fields are optional to support partial configuration and fallback chains.
 * 
 * Hierarchy resolution (backend automatically applies):
 * 1. Request-level override (user specified for this request)
 * 2. Per-model defaults (stored in model.inferenceDefaults)
 * 3. Global settings (stored in AppSettings.inferenceDefaults)
 * 4. Hardcoded fallback (e.g., temperature = 0.7)
 */
export interface InferenceConfig {
  /** Sampling temperature (0.0 - 2.0). Controls randomness. */
  temperature?: number;
  /** Nucleus sampling threshold (0.0 - 1.0). Cumulative probability cutoff. */
  topP?: number;
  /** Top-K sampling limit. Considers only K most likely tokens. */
  topK?: number;
  /** Maximum tokens to generate in response. */
  maxTokens?: number;
  /** Repetition penalty (> 0.0, typically 1.0 - 1.3). */
  repeatPenalty?: number;
}

// ============================================================================
// Model Types
// ============================================================================

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
  // Inference defaults
  inferenceDefaults?: InferenceConfig;
}

export interface DownloadConfig {
  repo_id: string;
  quantization?: string;
}

export interface ServeConfig {
  id: number;
  context_length?: number;
  mlock?: boolean;
  port?: number;
  jinja?: boolean;
  // Inference parameters for this serve session
  temperature?: number;
  top_p?: number;
  top_k?: number;
  max_tokens?: number;
  repeat_penalty?: number;
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
  llama_base_port?: number | null;
  max_download_queue_size?: number | null;
  title_generation_prompt?: string | null;
  show_memory_fit_indicators?: boolean | null;
  /** Maximum iterations for tool calling agentic loop (default: 25) */
  max_tool_iterations?: number | null;
  /** Maximum stagnation steps before stopping agent loop (default: 5) */
  max_stagnation_steps?: number | null;
  /** Default model ID for quick commands (e.g., `gglib question`) */
  default_model_id?: number | null;
  /** Global inference parameter defaults */
  inferenceDefaults?: InferenceConfig | null;
}

export interface UpdateSettingsRequest {
  default_download_path?: string | null | undefined;
  default_context_size?: number | null | undefined;
  proxy_port?: number | null | undefined;
  llama_base_port?: number | null | undefined;
  max_download_queue_size?: number | null | undefined;
  title_generation_prompt?: string | null | undefined;
  show_memory_fit_indicators?: boolean | null | undefined;
  /** Maximum iterations for tool calling agentic loop (default: 25) */
  max_tool_iterations?: number | null | undefined;
  /** Maximum stagnation steps before stopping agent loop (default: 5) */
  max_stagnation_steps?: number | null | undefined;
  /** Default model ID for quick commands (e.g., `gglib question`) */
  default_model_id?: number | null | undefined;
  /** Global inference parameter defaults */
  inferenceDefaults?: InferenceConfig | null | undefined;
}

// ============================================================================
// System Memory Types (for "Will it fit?" indicators)
// ============================================================================

/**
 * System memory information for model fit calculations.
 */
export interface SystemMemoryInfo {
  /** Total system RAM in bytes */
  totalRamBytes: number;
  /** GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon) */
  gpuMemoryBytes?: number | null;
  /** Whether the system has Apple Silicon with unified memory */
  isAppleSilicon: boolean;
  /** Whether the system has an NVIDIA GPU */
  hasNvidiaGpu: boolean;
}

/**
 * Fit status for a model quantization based on available memory.
 */
export type FitStatus = 'fits' | 'tight' | 'wont_fit' | 'unknown';

// ============================================================================
// Server Health Types (for monitoring server lifecycle)
// ============================================================================

/**
 * Health status for a running server.
 * Maps to gglib-core::ports::server_health::ServerHealthStatus.
 * Uses 'status' as discriminant to match Rust serde(tag = "status").
 */
export type ServerHealthStatus = 
  | { status: 'healthy' }
  | { status: 'degraded'; reason: string }
  | { status: 'unreachable'; lastError: string }
  | { status: 'processdied' };

/**
 * Get display info for a health status (dot, label, title).
 */
export function getHealthDisplay(health?: ServerHealthStatus): { dot: string; label: string; title: string } {
  if (!health) {
    return { dot: '🟡', label: 'Unknown', title: 'No health data yet' };
  }

  switch (health.status) {
    case 'healthy':
      return { dot: '🟢', label: 'Healthy', title: 'Server responding normally' };

    case 'degraded':
      return {
        dot: '🟡',
        label: 'Degraded',
        title: health.reason ?? 'Health checks reporting degraded state',
      };

    case 'unreachable':
      return {
        dot: '🔴',
        label: 'Unreachable',
        title: health.lastError ?? 'Health endpoint not reachable',
      };

    case 'processdied':
      return {
        dot: '🔴',
        label: 'Process died',
        title: 'Server process appears to have exited',
      };

    default:
      return { dot: '🟡', label: 'Unknown', title: 'Unrecognized health state' };
  }
}

// ============================================================================
// Download Types (re-exported from transport types for convenience)
// ============================================================================

export type {
  DownloadStatus,
  ShardInfo,
  DownloadQueueItem,
  DownloadQueueStatus,
  DownloadCompletionInfo,
} from '../services/transport/types/downloads';

export type {
  DownloadSummary,
  DownloadEvent,
} from '../services/transport/types/events';

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

/**
 * Response for tool/function calling support detection.
 */
export interface HfToolSupportResponse {
  /** Whether the model supports tool/function calling */
  supports_tool_calling: boolean;
  /** Confidence level of the detection (0.0 to 1.0) */
  confidence: number;
  /** Detected tool calling format (e.g., "hermes", "llama3", "mistral") */
  detected_format?: string | null;
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
