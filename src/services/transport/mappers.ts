/**
 * Transport layer mappers.
 * 
 * These functions map between frontend types and backend request DTOs,
 * serving as the single source of truth for request shape construction.
 */

import type { ServeConfig } from '../transport/types/models';
import type { SparseInferenceConfig } from '../../types';
import { INFERENCE_CONFIG_KEYS } from '../../constants/inferenceDefaults';

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
  /**
   * Sampling overrides for this session, read back through Rust's
   * `inference_params: Option<InferenceConfig>`.
   *
   * Typed as the config itself rather than re-listing its fields: the
   * inline object this replaced named nine of eighteen, and the nine it
   * omitted were accepted by the endpoint and dropped by the mapper.
   */
  inferenceParams?: SparseInferenceConfig;
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
  return {
    contextLength: config.contextLength,
    port: config.port,
    mlock: config.mlock ?? false,
    jinja: config.jinja,
    // reasoning_format is auto-detected from model tags on backend when omitted
    reasoningFormat: undefined,
    mtpDraftNMax: config.specDraftNMax,
    mtpDraftPMin: config.specDraftPMin,
    inferenceParams: inferenceParamsFrom(config),
  };
}

/**
 * The sampling half of a serve config, or nothing if the user set none of it.
 *
 * Two rules, both of which the hand-written version got wrong on some field:
 *
 * `!= null`, not `!== undefined` — a modal seeded from a model's stored
 * `inferenceDefaults` receives all eighteen fields as explicit nulls, because
 * that is what "nothing configured" looks like on the wire. Reading those as
 * chosen values sends an object of nulls where the omission is what lets the
 * backend resolve through its own layers.
 *
 * And it iterates {@link INFERENCE_CONFIG_KEYS} rather than naming fields, so
 * a parameter cannot be offered by a form and dropped here. Nine were.
 */
function inferenceParamsFrom(config: ServeConfig): SparseInferenceConfig | undefined {
  const set = INFERENCE_CONFIG_KEYS.filter((key) => config[key] != null);
  if (set.length === 0) return undefined;

  return Object.fromEntries(set.map((key) => [key, config[key]])) as SparseInferenceConfig;
}
