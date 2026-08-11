/**
 * Local models API module.
 * Handles local model CRUD operations, search, filtering, and system info.
 */

import { del, get, patch, post, put } from '../client';
import { TransportError } from '../../errors';
import type { ModelId } from '../../types/ids';
import type {
  GgufModel,
  ModelDetail,
  AddModelParams,
  UpdateModelParams,
  ModelFilterOptions,
  SystemMemoryInfo,
  ModelsDirectoryInfo,
  RetagResponse,
  SetCapabilitiesRequest,
  UpgradeCheck,
  UpgradeOutcome,
} from '../../types/models';
import type { SamplingExplanation } from '../../../../types';

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
 * Get full detail for a specific model by ID.
 * Returns a superset of GgufModel with HuggingFace provenance, timestamps, and raw GGUF metadata.
 * Returns null if not found (instead of throwing).
 */
export async function getModelDetail(id: ModelId): Promise<ModelDetail | null> {
  try {
    return await get<ModelDetail>(`/api/models/${id}/detail`);
  } catch (error) {
    if (TransportError.hasCode(error, 'NOT_FOUND')) {
      return null;
    }
    throw error;
  }
}

/**
 * Get a model's resolved sampling parameters and the layer that supplied each.
 *
 * `profile` names a configured inference profile to apply on top of the
 * model's own defaults; an unknown name is a 400 from the server rather than a
 * silent fall back to the unprofiled resolution.
 *
 * Returns null if the model is not found (instead of throwing).
 */
export async function explainModelSampling(
  id: ModelId,
  profile?: string,
): Promise<SamplingExplanation | null> {
  const query = profile ? `?profile=${encodeURIComponent(profile)}` : '';
  try {
    return await get<SamplingExplanation>(`/api/models/${id}/explain${query}`);
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
    quantization: params.quantization,
    filePath: params.filePath,
    inferenceDefaults: params.inferenceDefaults,
    serverDefaults: params.serverDefaults,
  });
}

/**
 * Re-run capability detection over a model's stored metadata
 * (`gglib model retag`). `full` rebuilds the system-tag namespace.
 */
export async function retagModel(modelId: number, full = false): Promise<RetagResponse> {
  return post<RetagResponse>(`/api/models/${modelId}/retag`, { full });
}

/** Set or clear a model's capability flags. Returns the updated model. */
export async function setModelCapabilities(
  modelId: number,
  request: SetCapabilitiesRequest,
): Promise<GgufModel> {
  return patch<GgufModel>(`/api/models/${modelId}/capabilities`, request);
}

/** Commit-SHA update check (`gglib model check-updates` for one model). */
export async function checkModelUpgrade(modelId: number): Promise<UpgradeCheck> {
  return get<UpgradeCheck>(`/api/models/${modelId}/upgrade-check`);
}

/**
 * Re-download at the latest HuggingFace revision (`gglib model upgrade`).
 * Blocking for the download's duration — callers show a busy state.
 */
export async function upgradeModel(modelId: number): Promise<UpgradeOutcome> {
  return post<UpgradeOutcome>(`/api/models/${modelId}/upgrade`, null);
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
  return get<SystemMemoryInfo>('/api/config/system/memory');
}

/**
 * Get models directory information.
 */
export async function getModelsDirectory(): Promise<ModelsDirectoryInfo> {
  return get<ModelsDirectoryInfo>('/api/config/system/models-directory');
}

/**
 * Set models directory path.
 */
export async function setModelsDirectory(path: string): Promise<void> {
  await put<void>('/api/config/system/models-directory', { path });
}
