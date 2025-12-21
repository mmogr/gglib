/**
 * Models client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/models
 */

import { getTransport } from '../transport';
import type { ModelId } from '../transport/types/ids';
import type {
  GgufModel,
  ModelFilterOptions,
} from '../../types';
import type {
  AddModelParams,
  UpdateModelParams,
  SearchModelsParams,
} from '../transport/types/models';

/**
 * List all models in the library.
 */
export async function listModels(): Promise<GgufModel[]> {
  return getTransport().listModels();
}

/**
 * Get a specific model by ID.
 * Returns null if model not found.
 */
export async function getModel(id: ModelId): Promise<GgufModel | null> {
  return getTransport().getModel(id);
}

/**
 * Add a model from a local file path.
 */
export async function addModel(params: AddModelParams): Promise<GgufModel> {
  return getTransport().addModel(params);
}

/**
 * Remove a model from the library.
 * Does not delete the underlying file.
 */
export async function removeModel(id: ModelId): Promise<void> {
  return getTransport().removeModel(id);
}

/**
 * Update model metadata (e.g., name).
 */
export async function updateModel(params: UpdateModelParams): Promise<GgufModel> {
  return getTransport().updateModel(params);
}

/**
 * Search models with filters.
 */
export async function searchModels(params: SearchModelsParams): Promise<GgufModel[]> {
  return getTransport().searchModels(params);
}

/**
 * Get available filter options based on current library contents.
 */
export async function getModelFilterOptions(): Promise<ModelFilterOptions> {
  return getTransport().getModelFilterOptions();
}
