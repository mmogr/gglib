/**
 * Transport layer mappers.
 * 
 * These functions map between frontend types and backend request DTOs,
 * serving as the single source of truth for request shape construction.
 */

import type { ServeConfig } from '../transport/types/models';

/**
 * Request shape matching Rust's StartServerRequest.
 * Must stay in sync with gglib-app-services/src/types.rs::StartServerRequest
 */
export interface StartServerRequest {
  contextLength?: number;
  port?: number;
  mlock: boolean;
  jinja?: boolean;
  reasoningFormat?: string;
  /** Number of MTP draft tokens. undefined = auto; 0 = disable. Matches Rust mtp_draft_n_max. */
  mtpDraftNMax?: number;
  /** Minimum acceptance probability for MTP draft tokens. Matches Rust mtp_draft_p_min. */
  mtpDraftPMin?: number;
  // Inference parameters as nested object (matches Rust's inference_params field)
  inferenceParams?: {
    temperature?: number;
    topP?: number;
    topK?: number;
    maxTokens?: number;
    repeatPenalty?: number;
  };
}

/**
 * Convert ServeConfig to StartServerRequest for Tauri IPC.
 * 
 * Extracts only the fields needed by StartServerRequest, omitting `id` 
 * (which is passed separately) and legacy `ctx_size` (replaced by context_length).
 * 
 * @param config - Frontend serve configuration
 * @returns StartServerRequest matching Rust type
 */
export function toStartServerRequest(config: ServeConfig): StartServerRequest {
  // Build inference params object only if any values are set
  const hasInferenceParams = config.temperature !== undefined || 
    config.topP !== undefined || 
    config.topK !== undefined || 
    config.maxTokens !== undefined || 
    config.repeatPenalty !== undefined;
  
  const inferenceParams = hasInferenceParams ? {
    temperature: config.temperature,
    topP: config.topP,
    topK: config.topK,
    maxTokens: config.maxTokens,
    repeatPenalty: config.repeatPenalty,
  } : undefined;

  return {
    contextLength: config.contextLength,
    port: config.port,
    mlock: config.mlock ?? false,
    jinja: config.jinja,
    // reasoning_format is auto-detected from model tags on backend when omitted
    reasoningFormat: undefined,
    mtpDraftNMax: config.specDraftNMax,
    mtpDraftPMin: config.specDraftPMin,
    inferenceParams,
  };
}
