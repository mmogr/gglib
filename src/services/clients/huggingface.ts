/**
 * HuggingFace client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/huggingface
 */

import { getTransport } from '../transport';
import type { HfModelId } from '../transport/types/ids';
import type {
  HfSearchRequest,
  HfSearchResponse,
  HfQuantizationsResponse,
  HfToolSupportResponse,
} from '../../types';

/**
 * Browse/search HuggingFace models.
 */
export async function browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse> {
  return getTransport().browseHfModels(params);
}

/**
 * Get available quantizations for a HuggingFace model.
 */
export async function getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse> {
  return getTransport().getHfQuantizations(modelId);
}

/**
 * Get tool support information for a HuggingFace model.
 */
export async function getHfToolSupport(modelId: HfModelId): Promise<HfToolSupportResponse> {
  return getTransport().getHfToolSupport(modelId);
}
