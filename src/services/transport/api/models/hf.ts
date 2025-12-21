/**
 * HuggingFace models API module.
 * Handles browsing and querying HuggingFace model hub.
 */

import { get, post } from '../client';
import { HF_SEARCH_PATH, HF_QUANTIZATIONS_PATH, HF_TOOL_SUPPORT_PATH } from '../../../api/routes';
import type { HfModelId } from '../../types/ids';
import type {
  HfSearchRequest,
  HfSearchResponse,
  HfQuantizationsResponse,
  HfToolSupportResponse,
} from '../../types/models';

/**
 * Browse HuggingFace models with search and filtering.
 */
export async function browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
  return post<HfSearchResponse>(HF_SEARCH_PATH, params);
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
