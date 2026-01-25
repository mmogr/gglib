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
  HfModelSummary,
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
 * Get model summary by exact repo ID (direct API lookup).
 *
 * Unlike browseHfModels (search), this fetches model info directly
 * using the exact repo ID (e.g., "unsloth/medgemma-4b-it-GGUF").
 */
export async function getHfModelSummary(modelId: HfModelId): Promise<HfModelSummary> {
  return getTransport().getHfModelSummary(modelId);
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
