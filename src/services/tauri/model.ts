// Model domain operations
// CRUD for local GGUF models in the library

import { GgufModel, ModelFilterOptions } from "../../types";
import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

/**
 * List all models in the local library.
 */
export async function listModels(): Promise<GgufModel[]> {
  if (isTauriApp) {
    return await tauriInvoke<GgufModel[]>('list_models');
  } else {
    const response = await apiFetch(`/models`);
    if (!response.ok) {
      throw new Error(`Failed to fetch models: ${response.statusText}`);
    }
    const data: ApiResponse<GgufModel[]> = await response.json();
    return data.data || [];
  }
}

/**
 * Get a single model by ID.
 */
export async function getModel(id: number): Promise<GgufModel> {
  if (isTauriApp) {
    const models = await listModels();
    const model = models.find(m => m.id === id);
    if (!model) throw new Error(`Model ${id} not found`);
    return model;
  } else {
    const response = await apiFetch(`/models/${id}`);
    if (!response.ok) {
      throw new Error(`Failed to fetch model: ${response.statusText}`);
    }
    const data: ApiResponse<GgufModel> = await response.json();
    if (!data.data) {
      throw new Error(`Model ${id} not found`);
    }
    return data.data;
  }
}

/**
 * Add a model from a local file path.
 */
export async function addModel(filePath: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('add_model', { filePath });
  } else {
    const response = await apiFetch(`/models`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ file_path: filePath }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to add model');
    }

    const data: ApiResponse<GgufModel> = await response.json();
    return `Model added: ${data.data?.name}`;
  }
}

/**
 * Remove a model from the library.
 */
export async function removeModel(identifier: string, force: boolean = false): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('remove_model', { identifier, force });
  } else {
    const response = await apiFetch(`/models/${identifier}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ force }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to remove model');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Model removed successfully';
  }
}

/**
 * Update model metadata.
 */
export async function updateModel(
  id: number,
  updates: {
    name?: string;
    quantization?: string;
    file_path?: string;
  }
): Promise<GgufModel> {
  if (isTauriApp) {
    return await tauriInvoke<GgufModel>('update_model', { id, updates });
  } else {
    const response = await apiFetch(`/models/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(updates),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to update model');
    }

    const data: ApiResponse<GgufModel> = await response.json();
    if (!data.data) {
      throw new Error('Invalid response from server');
    }
    return data.data;
  }
}

/**
 * Search for models on HuggingFace.
 */
export async function searchModels(query: string, limit: number = 20): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('search_models', {
      query,
      limit,
      sort: "downloads",
      ggufOnly: true,
    });
  } else {
    const response = await apiFetch(`/models/search?query=${encodeURIComponent(query)}&limit=${limit}`);
    if (!response.ok) {
      throw new Error(`Failed to search models: ${response.statusText}`);
    }
    const data: ApiResponse<string> = await response.json();
    return data.data || 'Search completed';
  }
}

/**
 * Get filter options for the model library UI.
 */
export async function getModelFilterOptions(): Promise<ModelFilterOptions> {
  if (isTauriApp) {
    return await tauriInvoke<ModelFilterOptions>('get_model_filter_options');
  } else {
    const response = await apiFetch(`/models/filter-options`);
    if (!response.ok) {
      throw new Error(`Failed to fetch filter options: ${response.statusText}`);
    }
    const data: ApiResponse<ModelFilterOptions> = await response.json();
    return data.data || { quantizations: [], param_range: null, context_range: null };
  }
}
