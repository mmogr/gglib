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
    presencePenalty?: number;
    minP?: number;
    reasoningEffort?: string;
    reasoningBudgetTokens?: number;
  };
}

/**
 * Convert ServeConfig to StartServerRequest.
 *
 * Neither caller sends `id` inside the object, which is why it is not here:
 * `POST /api/servers/start` spreads the result flat beside an `id`, read back
 * through `StartServerBody`'s `#[serde(flatten)]`, and
 * `POST /api/proxy/start-pinned` nests it under `options`.
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
    config.repeatPenalty !== undefined ||
    config.presencePenalty !== undefined ||
    config.minP !== undefined ||
    // Both reasoning controls count towards "any values are set". Without
    // these two lines a session that named *only* a reasoning level would
    // build no `inferenceParams` object at all, and the level would be
    // dropped by the one condition meant to decide whether to send it.
    config.reasoningEffort !== undefined ||
    config.reasoningBudgetTokens !== undefined;

  const inferenceParams = hasInferenceParams ? {
    temperature: config.temperature,
    topP: config.topP,
    topK: config.topK,
    maxTokens: config.maxTokens,
    repeatPenalty: config.repeatPenalty,
    presencePenalty: config.presencePenalty,
    minP: config.minP,
    reasoningEffort: config.reasoningEffort,
    reasoningBudgetTokens: config.reasoningBudgetTokens,
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
