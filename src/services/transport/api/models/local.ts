/**
 * Local models API module.
 * Handles local model CRUD operations, search, filtering, and system info.
 */

import { get, post, put, del } from '../client';
import { TransportError } from '../../errors';
import type { ModelId } from '../../types/ids';
import type {
  GgufModel,
  AddModelParams,
  UpdateModelParams,
  SearchModelsParams,
  ModelFilterOptions,
  SystemMemoryInfo,
  ModelsDirectoryInfo,
} from '../../types/models';

/**
 * List all local models.
 */
export async function listModels(): Promise<GgufModel[]> {
  return get<GgufModel[]>('/api/models');
}

/**
 * Get a specific model by ID.
 * Returns null if not found (instead of throwing).
 */
export async function getModel(id: ModelId): Promise<GgufModel | null> {
  try {
    return await get<GgufModel>(`/api/models/${id}`);
  } catch (error) {
    if (TransportError.hasCode(error, 'NOT_FOUND')) {
      return null;
    }
    throw error;
  }
}

/**
 * Add a new model from a local file.
 */
export async function addModel(params: AddModelParams): Promise<GgufModel> {
  return post<GgufModel>('/api/models', {
    file_path: params.filePath,
    name: params.name,
  });
}

/**
 * Remove a model.
 */
export async function removeModel(id: ModelId): Promise<void> {
  await del<void>(`/api/models/${id}`, { force: false });
}

/**
 * Update model metadata.
 */
export async function updateModel(params: UpdateModelParams): Promise<GgufModel> {
  return put<GgufModel>(`/api/models/${params.id}`, {
    name: params.name,
    inferenceDefaults: params.inferenceDefaults,
  });
}

/**
 * Search local models with filters.
 */
export async function searchModels(params: SearchModelsParams): Promise<GgufModel[]> {
  const queryParams = new URLSearchParams();
  
  if (params.query) {
    queryParams.set('query', params.query);
  }
  if (params.tags?.length) {
    queryParams.set('tags', params.tags.join(','));
  }
  if (params.quantizations?.length) {
    queryParams.set('quantizations', params.quantizations.join(','));
  }
  if (params.minParams !== undefined) {
    queryParams.set('min_params', String(params.minParams));
  }
  if (params.maxParams !== undefined) {
    queryParams.set('max_params', String(params.maxParams));
  }
  
  const queryString = queryParams.toString();
  const path = queryString ? `/api/models/search?${queryString}` : '/api/models/search';
  
  return get<GgufModel[]>(path);
}

/**
 * Get available filter options (tags, quantizations, parameter ranges).
 */
export async function getModelFilterOptions(): Promise<ModelFilterOptions> {
  return get<ModelFilterOptions>('/api/models/filter-options');
}

/**
 * Get system memory information.
 */
export async function getSystemMemory(): Promise<SystemMemoryInfo | null> {
  return get<SystemMemoryInfo>('/api/system/memory');
}

/**
 * Get models directory information.
 */
export async function getModelsDirectory(): Promise<ModelsDirectoryInfo> {
  return get<ModelsDirectoryInfo>('/api/system/models-directory');
}

/**
 * Set models directory path.
 */
export async function setModelsDirectory(path: string): Promise<void> {
  await put<void>('/api/system/models-directory', { path });
}
