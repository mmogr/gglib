/**
 * HuggingFace models API module.
 * Handles browsing and querying HuggingFace model hub.
 */

import { get, post } from '../client';
import { HUB_READ_LIMIT_MS } from '../timeLimit';
import type { HfModelId } from '../../types/ids';
import type {
  HfModelSummary,
  HfPickResponse,
  HfQuantizationsResponse,
  HfSearchRequest,
  HfSearchResponse,
} from '../../../../types';

/**
 * Browse HuggingFace models with search and filtering.
 *
 * The reads below are answered from the Hub, which can take minutes where a
 * read of the daemon's own takes moments: they are held to the Hub's limit
 * (`HUB_READ_LIMIT_MS`) rather than 30 s. Each takes a `signal`, since a read
 * nobody is waiting for still holds one of the page's connections to the
 * daemon until it ends.
 */
export async function browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
  return post<HfSearchResponse>('/api/models/hf/search', params);
}

/**
 * Get model summary by exact repo ID (direct API lookup).
 * 
 * Unlike browseHfModels (search), this fetches model info directly
 * using the exact repo ID (e.g., "unsloth/medgemma-4b-it-GGUF").
 * 
 * The model ID is NOT URL-encoded here because the backend uses a 
 * wildcard route (/hf/model/*model_id) that captures the full path
 * including the slash.
 */
export async function getHfModelSummary(modelId: HfModelId, signal?: AbortSignal): Promise<HfModelSummary> {
  // Don't encode the modelId - the wildcard route expects the raw path
  return get<HfModelSummary>(`/api/models/hf/model/${modelId}`, { timeoutMs: HUB_READ_LIMIT_MS, signal });
}

/**
 * Get available quantizations for a HuggingFace model.
 */
export async function getHfQuantizations(modelId: HfModelId, signal?: AbortSignal): Promise<HfQuantizationsResponse> {
  return get<HfQuantizationsResponse>(
    `/api/models/hf/quantizations/${encodeURIComponent(modelId)}`,
    { timeoutMs: HUB_READ_LIMIT_MS, signal },
  );
}

/**
 * The quantization a download of a HuggingFace model takes when none is
 * asked for, as the download picks it. Slower than the listing: it reads
 * the headers the listing's heads stop short of.
 */
export async function getHfPick(modelId: HfModelId, signal?: AbortSignal): Promise<HfPickResponse> {
  return get<HfPickResponse>(`/api/models/hf/pick/${encodeURIComponent(modelId)}`, { timeoutMs: HUB_READ_LIMIT_MS, signal });
}
