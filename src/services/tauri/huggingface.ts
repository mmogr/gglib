// HuggingFace browser domain operations
// Browse and inspect models on HuggingFace

import {
  HfSearchRequest,
  HfSearchResponse,
  HfQuantizationsResponse,
  HfToolSupportResponse,
} from "../../types";
import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

/**
 * Browse HuggingFace models with search, parameter filtering, and pagination.
 * Returns GGUF text-generation models only.
 */
export async function browseHfModels(request: HfSearchRequest): Promise<HfSearchResponse> {
  if (isTauriApp) {
    return await tauriInvoke<HfSearchResponse>('browse_hf_models', { request });
  } else {
    const response = await apiFetch(`/hf/browse`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to browse HuggingFace models');
    }

    const data: ApiResponse<HfSearchResponse> = await response.json();
    if (!data.data) {
      throw new Error('Invalid response from server');
    }
    return data.data;
  }
}

/**
 * Get available quantizations for a HuggingFace model.
 * Returns detailed info about each quantization including file size and sharding.
 */
export async function getHfQuantizations(modelId: string): Promise<HfQuantizationsResponse> {
  if (isTauriApp) {
    return await tauriInvoke<HfQuantizationsResponse>('get_hf_quantizations', { modelId });
  } else {
    // URL encode the model_id since it contains a slash
    const encodedModelId = encodeURIComponent(modelId);
    const response = await apiFetch(`/hf/quantizations/${encodedModelId}`);

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to get model quantizations');
    }

    const data: ApiResponse<HfQuantizationsResponse> = await response.json();
    if (!data.data) {
      throw new Error('Invalid response from server');
    }
    return data.data;
  }
}

/**
 * Check if a HuggingFace model supports tool/function calling.
 * Analyzes the model's chat template using unified detection logic.
 */
export async function getHfToolSupport(modelId: string): Promise<HfToolSupportResponse> {
  if (isTauriApp) {
    return await tauriInvoke<HfToolSupportResponse>('get_hf_tool_support', { modelId });
  } else {
    // URL encode the model_id since it contains a slash
    const encodedModelId = encodeURIComponent(modelId);
    const response = await apiFetch(`/hf/tool-support/${encodedModelId}`);

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to get tool support info');
    }

    const data: ApiResponse<HfToolSupportResponse> = await response.json();
    if (!data.data) {
      throw new Error('Invalid response from server');
    }
    return data.data;
  }
}
