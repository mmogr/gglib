/**
 * HuggingFace models API module.
 * Handles browsing and querying HuggingFace model hub.
 */

import { get, post } from '../client';
import { HF_SEARCH_PATH, HF_MODEL_PATH, HF_QUANTIZATIONS_PATH, HF_TOOL_SUPPORT_PATH } from '../../../api/routes';
import type { HfModelId } from '../../types/ids';
import type {
  HfSearchRequest,
  HfSearchResponse,
  HfQuantizationsResponse,
  HfToolSupportResponse,
} from '../../types/models';
import type { HfModelSummary } from '../../../../types';

/**
 * Browse HuggingFace models with search and filtering.
 */
export async function browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
  return post<HfSearchResponse>(HF_SEARCH_PATH, params);
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
export async function getHfModelSummary(modelId: HfModelId): Promise<HfModelSummary> {
  // Don't encode the modelId - the wildcard route expects the raw path
  return get<HfModelSummary>(`${HF_MODEL_PATH}/${modelId}`);
}

/**
 * Get available quantizations for a HuggingFace model.
 */
export async function getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse> {
  return get<HfQuantizationsResponse>(
    `${HF_QUANTIZATIONS_PATH}/${encodeURIComponent(modelId)}`
  );
}

/**
 * Get tool support information for a HuggingFace model.
 */
export async function getHfToolSupport(modelId: HfModelId): Promise<HfToolSupportResponse> {
  return get<HfToolSupportResponse>(
    `${HF_TOOL_SUPPORT_PATH}/${encodeURIComponent(modelId)}`
  );
}
